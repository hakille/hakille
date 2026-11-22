//! Orchestrateur domotique souverain `SovereignHomeHub` avec persistance `redb`,
//! scènes réactives, flux d'événements temps réel et exécution auditable.

use crate::automation::{AutomationEngine, AutomationEvaluationReport, AutomationRule};
use crate::digital_twin::HomeDigitalTwin;
use crate::energy::{
    DailyEnergyReport, EnergyAnomaly, EnergyBalance, EnergyDialecticReport,
    EnergyHomeostasisEngine, EnergySample, TariffPlan, compute_daily_kwh_and_cost,
};
use crate::intent::{ResolvedHomeIntent, resolve_home_intent};
use crate::mesh_sync::{
    HomeMeshSyncReport, HomeTwinSyncDelta, build_sync_delta, merge_devices_lww,
};
use crate::model::{
    CommandReceipt, ConnectionStatus, DiscoveryRecord, HomeCommand, HomeDevice, HomeError,
    HomeEvent, HomeResult, HomeRiskTier, ProtocolCapabilityInfo, ProtocolKind, RoomZone,
};
use crate::protocols::{discover_local, dispatch_command_real, protocol_catalog};
use crate::security::{
    HomeBiscuitPolicy, HomeLanGuard, HomeResidentRole, HomeRiskClassifier, PhysicalHoldProof,
};
use chrono::Timelike;
use hakille_core::CapabilityToken;
use hakille_mesh::MeshFrame;
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, broadcast};

const DEVICES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("home_devices");
const ROOMS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("home_rooms");
const SCENES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("home_scenes");
const RECEIPTS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("home_receipts");
const AUTOMATIONS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("home_automations");
const ENERGY_SAMPLES_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("home_energy_samples");

/// Scène ou automatisation locale (ex: "Mode Nuit", "Départ Maison", "Surplus Solaire").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HomeScene {
    pub id: String,
    pub name: String,
    pub description: String,
    pub steps: Vec<(String, HomeCommand)>,
}

/// Prévisualisation Dry-Run d'une commande domotique (zéro effet physique tant que non approuvé).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HomeDryRunPreview {
    pub device_id: String,
    pub device_name: String,
    pub protocol: ProtocolKind,
    pub risk_tier: HomeRiskTier,
    pub requires_600ms_hold: bool,
    pub endpoint_safe_lan: bool,
    pub summary: String,
}

/// Hub Domotique Souverain centralisé.
pub struct SovereignHomeHub {
    twin: RwLock<HomeDigitalTwin>,
    scenes: RwLock<BTreeMap<String, HomeScene>>,
    automations: RwLock<BTreeMap<String, AutomationRule>>,
    energy_samples: RwLock<Vec<EnergySample>>,
    tariff_plan: RwLock<TariffPlan>,
    receipts: RwLock<Vec<CommandReceipt>>,
    discoveries: RwLock<Vec<DiscoveryRecord>>,
    db: Option<Arc<Database>>,
    lan_guard: HomeLanGuard,
    energy_engine: EnergyHomeostasisEngine,
    events_tx: broadcast::Sender<HomeEvent>,
}

impl SovereignHomeHub {
    /// Crée un hub en mémoire (zéro appareil fictif par défaut — contrat d'honnêteté).
    pub fn new_memory() -> Self {
        let (events_tx, _) = broadcast::channel(128);
        Self {
            twin: RwLock::new(HomeDigitalTwin::new()),
            scenes: RwLock::new(BTreeMap::new()),
            automations: RwLock::new(BTreeMap::new()),
            energy_samples: RwLock::new(Vec::new()),
            tariff_plan: RwLock::new(TariffPlan::default()),
            receipts: RwLock::new(Vec::new()),
            discoveries: RwLock::new(Vec::new()),
            db: None,
            lan_guard: HomeLanGuard::new(),
            energy_engine: EnergyHomeostasisEngine::default(),
            events_tx,
        }
    }

    /// Ouvre ou crée la base `redb` (`<data_dir>/home.redb`) et recharge appareils, pièces, scènes, règles et échantillons.
    pub async fn open(db_path: impl AsRef<Path>) -> HomeResult<Self> {
        let path = db_path.as_ref();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|e| HomeError::Storage(e.to_string()))?;
        }
        let db = Arc::new(Database::create(path).map_err(|e| HomeError::Storage(e.to_string()))?);
        let write_txn = db
            .begin_write()
            .map_err(|e| HomeError::Storage(e.to_string()))?;
        {
            let _ = write_txn.open_table(DEVICES_TABLE);
            let _ = write_txn.open_table(ROOMS_TABLE);
            let _ = write_txn.open_table(SCENES_TABLE);
            let _ = write_txn.open_table(RECEIPTS_TABLE);
            let _ = write_txn.open_table(AUTOMATIONS_TABLE);
            let _ = write_txn.open_table(ENERGY_SAMPLES_TABLE);
        }
        write_txn
            .commit()
            .map_err(|e| HomeError::Storage(e.to_string()))?;

        let mut twin = HomeDigitalTwin::new();
        let mut scenes = BTreeMap::new();
        let mut automations = BTreeMap::new();
        let mut energy_samples = Vec::new();
        let mut receipts = Vec::new();

        let read_txn = db
            .begin_read()
            .map_err(|e| HomeError::Storage(e.to_string()))?;
        if let Ok(tbl) = read_txn.open_table(ROOMS_TABLE) {
            for item in tbl.iter().map_err(|e| HomeError::Storage(e.to_string()))? {
                let (_k, v) = item.map_err(|e| HomeError::Storage(e.to_string()))?;
                if let Ok(room) = serde_json::from_slice::<RoomZone>(v.value()) {
                    twin.upsert_room(room);
                }
            }
        }
        if let Ok(tbl) = read_txn.open_table(DEVICES_TABLE) {
            for item in tbl.iter().map_err(|e| HomeError::Storage(e.to_string()))? {
                let (_k, v) = item.map_err(|e| HomeError::Storage(e.to_string()))?;
                if let Ok(dev) = serde_json::from_slice::<HomeDevice>(v.value()) {
                    twin.upsert_device(dev);
                }
            }
        }
        if let Ok(tbl) = read_txn.open_table(SCENES_TABLE) {
            for item in tbl.iter().map_err(|e| HomeError::Storage(e.to_string()))? {
                let (_k, v) = item.map_err(|e| HomeError::Storage(e.to_string()))?;
                if let Ok(sc) = serde_json::from_slice::<HomeScene>(v.value()) {
                    scenes.insert(sc.id.clone(), sc);
                }
            }
        }
        if let Ok(tbl) = read_txn.open_table(RECEIPTS_TABLE) {
            for item in tbl.iter().map_err(|e| HomeError::Storage(e.to_string()))? {
                let (_k, v) = item.map_err(|e| HomeError::Storage(e.to_string()))?;
                if let Ok(r) = serde_json::from_slice::<CommandReceipt>(v.value()) {
                    receipts.push(r);
                }
            }
        }
        if let Ok(tbl) = read_txn.open_table(AUTOMATIONS_TABLE) {
            for item in tbl.iter().map_err(|e| HomeError::Storage(e.to_string()))? {
                let (_k, v) = item.map_err(|e| HomeError::Storage(e.to_string()))?;
                if let Ok(rule) = serde_json::from_slice::<AutomationRule>(v.value()) {
                    automations.insert(rule.id.clone(), rule);
                }
            }
        }
        if let Ok(tbl) = read_txn.open_table(ENERGY_SAMPLES_TABLE) {
            for item in tbl.iter().map_err(|e| HomeError::Storage(e.to_string()))? {
                let (_k, v) = item.map_err(|e| HomeError::Storage(e.to_string()))?;
                if let Ok(sample) = serde_json::from_slice::<EnergySample>(v.value()) {
                    energy_samples.push(sample);
                }
            }
        }

        let (events_tx, _) = broadcast::channel(128);
        Ok(Self {
            twin: RwLock::new(twin),
            scenes: RwLock::new(scenes),
            automations: RwLock::new(automations),
            energy_samples: RwLock::new(energy_samples),
            tariff_plan: RwLock::new(TariffPlan::default()),
            receipts: RwLock::new(receipts),
            discoveries: RwLock::new(Vec::new()),
            db: Some(db),
            lan_guard: HomeLanGuard::new(),
            energy_engine: EnergyHomeostasisEngine::default(),
            events_tx,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<HomeEvent> {
        self.events_tx.subscribe()
    }

    pub fn protocol_catalog(&self) -> Vec<ProtocolCapabilityInfo> {
        protocol_catalog()
    }

    pub async fn upsert_room(&self, room: RoomZone) -> HomeResult<()> {
        if let Some(db) = &self.db {
            let payload =
                serde_json::to_vec(&room).map_err(|e| HomeError::Storage(e.to_string()))?;
            let tx = db
                .begin_write()
                .map_err(|e| HomeError::Storage(e.to_string()))?;
            {
                let mut tbl = tx
                    .open_table(ROOMS_TABLE)
                    .map_err(|e| HomeError::Storage(e.to_string()))?;
                tbl.insert(room.id.as_str(), payload.as_slice())
                    .map_err(|e| HomeError::Storage(e.to_string()))?;
            }
            tx.commit().map_err(|e| HomeError::Storage(e.to_string()))?;
        }
        self.twin.write().await.upsert_room(room);
        Ok(())
    }

    /// Enregistre ou met à jour un appareil après validation stricte par `HomeLanGuard`.
    pub async fn register_device(&self, device: HomeDevice) -> HomeResult<()> {
        self.lan_guard.validate_endpoint(&device.endpoint)?;
        if let Some(db) = &self.db {
            let payload =
                serde_json::to_vec(&device).map_err(|e| HomeError::Storage(e.to_string()))?;
            let tx = db
                .begin_write()
                .map_err(|e| HomeError::Storage(e.to_string()))?;
            {
                let mut tbl = tx
                    .open_table(DEVICES_TABLE)
                    .map_err(|e| HomeError::Storage(e.to_string()))?;
                tbl.insert(device.id.as_str(), payload.as_slice())
                    .map_err(|e| HomeError::Storage(e.to_string()))?;
            }
            tx.commit().map_err(|e| HomeError::Storage(e.to_string()))?;
        }
        self.twin.write().await.upsert_device(device.clone());
        let _ = self
            .events_tx
            .send(HomeEvent::DeviceRegistered(Box::new(device)));
        Ok(())
    }

    pub async fn upsert_scene(&self, scene: HomeScene) -> HomeResult<()> {
        if let Some(db) = &self.db {
            let payload =
                serde_json::to_vec(&scene).map_err(|e| HomeError::Storage(e.to_string()))?;
            let tx = db
                .begin_write()
                .map_err(|e| HomeError::Storage(e.to_string()))?;
            {
                let mut tbl = tx
                    .open_table(SCENES_TABLE)
                    .map_err(|e| HomeError::Storage(e.to_string()))?;
                tbl.insert(scene.id.as_str(), payload.as_slice())
                    .map_err(|e| HomeError::Storage(e.to_string()))?;
            }
            tx.commit().map_err(|e| HomeError::Storage(e.to_string()))?;
        }
        self.scenes.write().await.insert(scene.id.clone(), scene);
        Ok(())
    }

    pub async fn list_devices(&self) -> Vec<HomeDevice> {
        self.twin.read().await.devices.values().cloned().collect()
    }

    pub async fn list_rooms(&self) -> Vec<RoomZone> {
        self.twin.read().await.rooms.values().cloned().collect()
    }

    pub async fn list_scenes(&self) -> Vec<HomeScene> {
        self.scenes.read().await.values().cloned().collect()
    }

    pub async fn list_receipts(&self) -> Vec<CommandReceipt> {
        self.receipts.read().await.clone()
    }

    pub async fn get_device(&self, device_id: &str) -> Option<HomeDevice> {
        self.twin.read().await.devices.get(device_id).cloned()
    }

    /// Lance une découverte LAN réelle (mDNS, SSDP, ONVIF) et mémorise les résultats observés.
    pub async fn discover_lan(
        &self,
        filter: Option<ProtocolKind>,
        timeout: Duration,
    ) -> HomeResult<Vec<DiscoveryRecord>> {
        let found = discover_local(filter, timeout).await?;
        let mut guard = self.discoveries.write().await;
        for rec in &found {
            let _ = self
                .events_tx
                .send(HomeEvent::DeviceDiscovered(rec.clone()));
            guard.push(rec.clone());
        }
        Ok(found)
    }

    /// Prévisualise l'impact d'une commande domotique sans l'exécuter physiquement.
    pub async fn preview_command(
        &self,
        device_id: &str,
        command: &HomeCommand,
    ) -> HomeResult<HomeDryRunPreview> {
        let dev = self
            .get_device(device_id)
            .await
            .ok_or_else(|| HomeError::DeviceNotFound(device_id.to_string()))?;
        let tier = HomeRiskClassifier::classify(dev.category, command);
        let endpoint_safe_lan = self.lan_guard.validate_endpoint(&dev.endpoint).is_ok();
        Ok(HomeDryRunPreview {
            device_id: dev.id.clone(),
            device_name: dev.name.clone(),
            protocol: dev.protocol,
            risk_tier: tier,
            requires_600ms_hold: tier == HomeRiskTier::Tier3CriticalExecution,
            endpoint_safe_lan,
            summary: format!(
                "{} '{}' [{} / {}] : action '{}' ({})",
                if tier == HomeRiskTier::Tier3CriticalExecution {
                    "[CRITIQUE 600ms]"
                } else {
                    "[LAN]"
                },
                dev.name,
                dev.protocol.as_str(),
                dev.room,
                command.action_name(),
                tier
            ),
        })
    }

    /// Exécute une commande domotique après vérification Biscuit + `HomeLanGuard` + preuve physique 600 ms pour Tier 3.
    pub async fn execute_authorized(
        &self,
        policy: &HomeBiscuitPolicy,
        token: &CapabilityToken,
        role: HomeResidentRole,
        device_id: &str,
        command: &HomeCommand,
        hold_proof: Option<&PhysicalHoldProof>,
    ) -> HomeResult<CommandReceipt> {
        let dev = self
            .get_device(device_id)
            .await
            .ok_or_else(|| HomeError::DeviceNotFound(device_id.to_string()))?;

        let tier = policy.verify_command(token, &dev, command, role)?;
        if tier == HomeRiskTier::Tier3CriticalExecution {
            let proof = hold_proof.ok_or_else(|| {
                HomeError::ConfirmationRequired(format!(
                    "'{}' sur '{}' exige un maintien physique >= 600 ms",
                    command.action_name(),
                    dev.name
                ))
            })?;
            proof.validate(None)?;
        }

        let res = dispatch_command_real(&dev, command, tier).await;
        match res {
            Ok(receipt) => {
                self.record_receipt(receipt.clone()).await?;
                self.apply_confirmed_state_update(device_id, command).await;
                let _ = self
                    .events_tx
                    .send(HomeEvent::CommandExecuted(receipt.clone()));
                Ok(receipt)
            }
            Err(e) => {
                // Mise à jour honnête du statut en cas d'indisponibilité réseau/matériel
                let mut twin = self.twin.write().await;
                if let Some(d) = twin.devices.get_mut(device_id) {
                    d.status = match &e {
                        HomeError::TransportUnavailable { reason, .. } => {
                            ConnectionStatus::UnsupportedTransport(reason.clone())
                        }
                        other => ConnectionStatus::Offline(other.to_string()),
                    };
                }
                Err(e)
            }
        }
    }

    async fn record_receipt(&self, receipt: CommandReceipt) -> HomeResult<()> {
        if let Some(db) = &self.db {
            let payload =
                serde_json::to_vec(&receipt).map_err(|e| HomeError::Storage(e.to_string()))?;
            let tx = db
                .begin_write()
                .map_err(|e| HomeError::Storage(e.to_string()))?;
            {
                let mut tbl = tx
                    .open_table(RECEIPTS_TABLE)
                    .map_err(|e| HomeError::Storage(e.to_string()))?;
                tbl.insert(receipt.receipt_id.as_str(), payload.as_slice())
                    .map_err(|e| HomeError::Storage(e.to_string()))?;
            }
            tx.commit().map_err(|e| HomeError::Storage(e.to_string()))?;
        }
        self.receipts.write().await.push(receipt);
        Ok(())
    }

    async fn apply_confirmed_state_update(&self, device_id: &str, command: &HomeCommand) {
        let mut twin = self.twin.write().await;
        if let Some(dev) = twin.devices.get_mut(device_id) {
            dev.status = ConnectionStatus::PairedOnline;
            match command {
                HomeCommand::SetPower { on } => dev.state.on = Some(*on),
                HomeCommand::Toggle => dev.state.on = Some(!dev.state.on.unwrap_or(false)),
                HomeCommand::SetLevel { level_pct } => {
                    dev.state.level_pct = Some(*level_pct);
                    dev.state.on = Some(*level_pct > 0);
                }
                HomeCommand::SetColorRgb { r, g, b } => dev.state.color_rgb = Some([*r, *g, *b]),
                HomeCommand::SetColorTemp { kelvin } => dev.state.color_temp_kelvin = Some(*kelvin),
                HomeCommand::SetTemperature { celsius } => {
                    dev.state.target_temperature_c = Some(*celsius)
                }
                HomeCommand::SetCoverPosition { position_pct } => {
                    dev.state.position_pct = Some(*position_pct)
                }
                HomeCommand::LockDoor => dev.state.locked = Some(true),
                HomeCommand::UnlockDoor => dev.state.locked = Some(false),
                HomeCommand::OpenGarage => dev.state.position_pct = Some(100),
                HomeCommand::CloseGarage => dev.state.position_pct = Some(0),
                HomeCommand::SetAlarm { mode, .. } => dev.state.alarm_mode = Some(mode.clone()),
                _ => {}
            }
            dev.state.updated_at = Some(chrono::Utc::now().to_rfc3339());
            let _ = self.events_tx.send(HomeEvent::StateUpdated {
                device_id: dev.id.clone(),
                protocol: dev.protocol,
                state: dev.state.clone(),
            });
        }
    }

    pub async fn energy_balance(&self) -> EnergyBalance {
        let twin = self.twin.read().await;
        self.energy_engine.compute_balance(twin.devices.values())
    }

    pub async fn energy_dialectic(&self) -> EnergyDialecticReport {
        let twin = self.twin.read().await;
        self.energy_engine.evaluate_arbitrage(twin.devices.values())
    }

    pub async fn export_palace_3d(&self) -> hakille_memory::PalaceLayoutEngine {
        self.twin.read().await.to_palace_3d()
    }

    pub async fn project_to_memory(&self, graph: &mut hakille_memory::KnowledgeGraph) -> usize {
        self.twin.read().await.project_to_knowledge_graph(graph)
    }

    /// Enregistre ou met à jour une règle d'automatisation réactive (persistée dans `home.redb`).
    pub async fn upsert_automation_rule(&self, rule: AutomationRule) -> HomeResult<()> {
        if let Some(db) = &self.db {
            let payload =
                serde_json::to_vec(&rule).map_err(|e| HomeError::Storage(e.to_string()))?;
            let tx = db
                .begin_write()
                .map_err(|e| HomeError::Storage(e.to_string()))?;
            {
                let mut tbl = tx
                    .open_table(AUTOMATIONS_TABLE)
                    .map_err(|e| HomeError::Storage(e.to_string()))?;
                tbl.insert(rule.id.as_str(), payload.as_slice())
                    .map_err(|e| HomeError::Storage(e.to_string()))?;
            }
            tx.commit().map_err(|e| HomeError::Storage(e.to_string()))?;
        }
        self.automations.write().await.insert(rule.id.clone(), rule);
        Ok(())
    }

    pub async fn list_automation_rules(&self) -> Vec<AutomationRule> {
        self.automations.read().await.values().cloned().collect()
    }

    /// Évalue les règles réactives (escalade obligatoire de toute action Tier 3 vers 600 ms).
    pub async fn evaluate_automations(&self) -> AutomationEvaluationReport {
        let devices = self.twin.read().await.devices.clone();
        let balance = self.energy_engine.compute_balance(devices.values());
        let mut rules_guard = self.automations.write().await;
        let mut list: Vec<AutomationRule> = rules_guard.values().cloned().collect();
        let report = AutomationEngine::evaluate(&mut list, &devices, &balance, chrono::Utc::now());
        for updated in list {
            rules_guard.insert(updated.id.clone(), updated);
        }
        report
    }

    /// Configure le plan tarifaire local (EDF Tempo, HP/HC ou tarif fixe).
    pub async fn set_tariff_plan(&self, plan: TariffPlan) {
        *self.tariff_plan.write().await = plan;
    }

    pub async fn tariff_plan(&self) -> TariffPlan {
        self.tariff_plan.read().await.clone()
    }

    /// Persiste un échantillon énergétique réel dans `home.redb`.
    pub async fn record_energy_sample(&self, sample: EnergySample) -> HomeResult<()> {
        if let Some(db) = &self.db {
            let payload =
                serde_json::to_vec(&sample).map_err(|e| HomeError::Storage(e.to_string()))?;
            let tx = db
                .begin_write()
                .map_err(|e| HomeError::Storage(e.to_string()))?;
            {
                let mut tbl = tx
                    .open_table(ENERGY_SAMPLES_TABLE)
                    .map_err(|e| HomeError::Storage(e.to_string()))?;
                tbl.insert(sample.timestamp_utc.as_str(), payload.as_slice())
                    .map_err(|e| HomeError::Storage(e.to_string()))?;
            }
            tx.commit().map_err(|e| HomeError::Storage(e.to_string()))?;
        }
        self.energy_samples.write().await.push(sample);
        Ok(())
    }

    /// Calcule le bilan journalier kWh/EUR réel sur les échantillons persistés.
    pub async fn daily_energy_report(&self) -> DailyEnergyReport {
        let samples = self.energy_samples.read().await.clone();
        let tariff = self.tariff_plan.read().await.clone();
        let hour = chrono::Utc::now().hour();
        compute_daily_kwh_and_cost(&samples, &tariff, hour)
    }

    /// Détecte les anomalies énergétiques à partir des sondes réelles.
    pub async fn detect_anomalies(&self) -> Vec<EnergyAnomaly> {
        let twin = self.twin.read().await;
        self.energy_engine.detect_anomalies(twin.devices.values())
    }

    /// Résout une commande naturelle FR/EN contre les appareils réellement enregistrés.
    pub async fn resolve_natural_intent(&self, utterance: &str) -> ResolvedHomeIntent {
        let twin = self.twin.read().await;
        let devices: Vec<HomeDevice> = twin.devices.values().cloned().collect();
        let rooms: Vec<RoomZone> = twin.rooms.values().cloned().collect();
        resolve_home_intent(utterance, &devices, &rooms)
    }

    /// Exporte un delta du jumeau domotique prêt à être chiffré via `hakille-mesh`.
    pub async fn export_mesh_delta(
        &self,
        origin_node_id: impl Into<String>,
        sequence: u64,
    ) -> HomeTwinSyncDelta {
        let twin = self.twin.read().await;
        let rooms: Vec<RoomZone> = twin.rooms.values().cloned().collect();
        let devices: Vec<HomeDevice> = twin.devices.values().cloned().collect();
        drop(twin);
        let scenes: Vec<HomeScene> = self.scenes.read().await.values().cloned().collect();
        let automations: Vec<AutomationRule> =
            self.automations.read().await.values().cloned().collect();
        build_sync_delta(
            origin_node_id,
            sequence,
            rooms,
            devices,
            scenes,
            automations,
        )
    }

    /// Vérifie (Ed25519 + ChaCha20-Poly1305) puis fusionne un `MeshFrame` dans le hub local.
    /// Tous les endpoints reçus sont re-validés par `HomeLanGuard` (zéro injection cloud).
    pub async fn apply_mesh_frame(
        &self,
        frame: &MeshFrame,
        symmetric_key: &[u8; 32],
    ) -> HomeResult<HomeMeshSyncReport> {
        let delta = HomeTwinSyncDelta::open_mesh_frame(frame, symmetric_key)?;
        for dev in &delta.devices {
            self.lan_guard.validate_endpoint(&dev.endpoint)?;
        }
        let rooms_merged = delta.rooms.len();
        for room in delta.rooms {
            self.upsert_room(room).await?;
        }
        let devices_merged = {
            let mut twin = self.twin.write().await;
            merge_devices_lww(&mut twin.devices, &delta.devices)
        };
        if let Some(db) = &self.db {
            let tx = db
                .begin_write()
                .map_err(|e| HomeError::Storage(e.to_string()))?;
            {
                let mut tbl = tx
                    .open_table(DEVICES_TABLE)
                    .map_err(|e| HomeError::Storage(e.to_string()))?;
                let twin = self.twin.read().await;
                for dev in &delta.devices {
                    if let Some(current) = twin.devices.get(&dev.id) {
                        let bytes = serde_json::to_vec(current)
                            .map_err(|e| HomeError::Storage(e.to_string()))?;
                        tbl.insert(current.id.as_str(), bytes.as_slice())
                            .map_err(|e| HomeError::Storage(e.to_string()))?;
                    }
                }
            }
            tx.commit().map_err(|e| HomeError::Storage(e.to_string()))?;
        }
        let scenes_merged = delta.scenes.len();
        for sc in delta.scenes {
            self.upsert_scene(sc).await?;
        }
        let automations_merged = delta.automations.len();
        for rule in delta.automations {
            self.upsert_automation_rule(rule).await?;
        }
        Ok(HomeMeshSyncReport {
            origin_node_id: delta.origin_node_id,
            frame_id: frame.frame_id.clone(),
            sequence: delta.sequence,
            rooms_merged,
            devices_merged,
            scenes_merged,
            automations_merged,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DeviceCategory;
    use crate::protocols::mqtt::{MqttPacket, read_mqtt_packet};
    use crate::security::new_ephemeral_authority;
    use chrono::Duration as ChronoDuration;
    use hakille_core::{CapabilityAction, CapabilityScope, Timestamp};
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn hub_redb_persistence_security_and_real_mqtt_execution() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("home.redb");

        // Serveur broker MQTT loopback réel
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let broker_addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let _ = read_mqtt_packet(&mut sock).await.unwrap();
            sock.write_all(
                &MqttPacket::ConnAck {
                    session_present: false,
                    reason_code: 0,
                }
                .to_bytes(),
            )
            .await
            .unwrap();
            if let MqttPacket::Publish { packet_id, .. } =
                read_mqtt_packet(&mut sock).await.unwrap()
            {
                sock.write_all(
                    &MqttPacket::PubAck {
                        packet_id: packet_id.unwrap(),
                    }
                    .to_bytes(),
                )
                .await
                .unwrap();
            }
        });

        let hub = SovereignHomeHub::open(&db_path).await.unwrap();
        hub.upsert_room(RoomZone::new("salon", "Salon", 0, [0.0, 1.4, -3.0]))
            .await
            .unwrap();
        hub.register_device(HomeDevice::new(
            "plug-salon",
            "Prise Salon",
            "salon",
            ProtocolKind::Mqtt,
            DeviceCategory::Switch,
            format!("mqtt://{broker_addr}"),
        ))
        .await
        .unwrap();

        let auth = new_ephemeral_authority();
        let policy = HomeBiscuitPolicy::new(auth.root_public_key());
        let token = HomeBiscuitPolicy::issue_home_token(
            &auth,
            vec![CapabilityScope::Network, CapabilityScope::Sensor],
            vec![CapabilityAction::Read, CapabilityAction::Execute],
            Timestamp(chrono::Utc::now() + ChronoDuration::minutes(10)),
            None,
            None,
            HomeResidentRole::Owner,
        )
        .unwrap();

        let receipt = hub
            .execute_authorized(
                &policy,
                &token,
                HomeResidentRole::Owner,
                "plug-salon",
                &HomeCommand::SetPower { on: true },
                None,
            )
            .await
            .unwrap();
        assert!(receipt.executed_on_wire);
        drop(hub);

        // Réouverture redb : l'appareil et son reçu persistent durablement
        let reopened = SovereignHomeHub::open(&db_path).await.unwrap();
        assert_eq!(reopened.list_devices().await.len(), 1);
        assert_eq!(reopened.list_receipts().await.len(), 1);
    }
}
