//! Réplique locale et boîte d'envoi hors ligne, sans transport réseau intégré.
//!
//! LWW choisit une seule valeur par clé : ce n'est pas un CRDT sans perte ni une
//! horloge causale distribuée. L'ordre total est (version, date, suppression,
//! octets). Les valeurs distantes concurrentes ou périmées différentes sont
//! conservées dans des conflits bornés ; une saturation refuse la mutation.
//! Les snapshots sérialisent l'état mais leur écriture atomique et durable
//! appartient à l'hôte. Aucun ACK n'est authentifié ici : le transport natif
//! doit authentifier son pair avant de confirmer une session ou un ACK.
//! Une restauration peut renvoyer une opération déjà reçue à distance : le pair
//! doit dédupliquer les UUID. Ce composant ne garantit pas une livraison exactly-once.

use hakille_core::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet, VecDeque};
use uuid::Uuid;

pub const MAX_SHADOW_VALUE_BYTES: usize = 256 * 1024;
pub const MAX_SHADOW_PENDING: usize = 256;
pub const MAX_SHADOW_CONFLICTS: usize = 128;
const MAX_KEYS: usize = 4096;
const MAX_SECTION_BYTES: usize = 4 * 1024 * 1024;
const MAX_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShadowState {
    Disconnected,
    Connecting,
    SyncingState,
    ActiveStreaming,
    SuspendedDozing,
    ReconnectingBackoff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowDelta {
    pub key: String,
    pub value: Vec<u8>,
    pub version: u64,
    pub timestamp: Timestamp,
    pub is_deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowDaemonConfig {
    /// Vide par défaut : stockage hors ligne, aucun fournisseur implicite.
    pub cluster_endpoint: String,
    /// Généré une fois par configuration ; sauvegarder/restaurer pour conserver l’identité.
    pub device_id: String,
    pub push_token: Option<String>,
    pub heartbeat_secs: u32,
}

impl Default for ShadowDaemonConfig {
    fn default() -> Self {
        Self {
            cluster_endpoint: String::new(),
            device_id: Uuid::new_v4().to_string(),
            push_token: None,
            heartbeat_secs: 30,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingShadowDelta {
    /// Identifiant d'opération stable après sauvegarde et reprise.
    pub id: Uuid,
    pub delta: ShadowDelta,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowConflict {
    pub retained_loser: ShadowDelta,
    pub materialized_winner: ShadowDelta,
}

#[derive(Debug, Clone)]
pub struct ShadowBatch {
    pub epoch: Uuid,
    pub items: Vec<PendingShadowDelta>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ShadowSnapshot {
    schema_version: u32,
    config: ShadowDaemonConfig,
    last_acknowledged_at: Option<Timestamp>,
    current_version: u64,
    pending: VecDeque<PendingShadowDelta>,
    edge_state: Vec<ShadowDelta>,
    conflicts: VecDeque<ShadowConflict>,
}

pub struct MobileShadowSync {
    config: ShadowDaemonConfig,
    state: ShadowState,
    last_acknowledged_at: Option<Timestamp>,
    pending_local_deltas: VecDeque<PendingShadowDelta>,
    received_shadow_deltas: VecDeque<ShadowDelta>,
    edge_state: BTreeMap<String, ShadowDelta>,
    conflicts: VecDeque<ShadowConflict>,
    current_version: u64,
    retry_count: u32,
    epoch: Option<Uuid>,
    prepared: HashSet<Uuid>,
    in_flight: HashSet<Uuid>,
}

fn validate_key(key: &str) -> Result<(), String> {
    if key.is_empty()
        || key.len() > 1024
        || key.contains(['\\', ':'])
        || key.chars().any(char::is_control)
        || key
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | "..") || part.len() > 255)
    {
        return Err("Clé shadow relative non canonique ou trop longue".into());
    }
    Ok(())
}

fn validate_delta(delta: &ShadowDelta) -> Result<(), String> {
    validate_key(&delta.key)?;
    if delta.version == 0
        || delta.value.len() > MAX_SHADOW_VALUE_BYTES
        || (delta.is_deleted && !delta.value.is_empty())
    {
        return Err("Delta shadow invalide : version, taille ou tombstone".into());
    }
    Ok(())
}

fn validate_config(config: &ShadowDaemonConfig) -> Result<(), String> {
    if config.device_id.is_empty()
        || config.device_id.len() > 128
        || config.device_id.chars().any(char::is_control)
        || config.cluster_endpoint.len() > 2048
        || config.cluster_endpoint.chars().any(char::is_control)
        || !(1..=3600).contains(&config.heartbeat_secs)
        || config
            .push_token
            .as_ref()
            .is_some_and(|token| token.len() > 4096)
    {
        return Err("Configuration shadow invalide ou hors limites".into());
    }
    Ok(())
}

fn validate_transport_config(config: &ShadowDaemonConfig) -> Result<(), String> {
    validate_config(config)?;
    if config.cluster_endpoint.trim().is_empty() {
        return Err("Endpoint shadow absent : configurer explicitement le transport hôte".into());
    }
    Ok(())
}

fn bytes(delta: &ShadowDelta) -> usize {
    delta.key.len() + delta.value.len()
}

fn compare(left: &ShadowDelta, right: &ShadowDelta) -> std::cmp::Ordering {
    (left.version, &left.timestamp, left.is_deleted, &left.value).cmp(&(
        right.version,
        &right.timestamp,
        right.is_deleted,
        &right.value,
    ))
}

impl MobileShadowSync {
    pub fn new(config: ShadowDaemonConfig) -> Self {
        Self {
            config,
            state: ShadowState::Disconnected,
            last_acknowledged_at: None,
            pending_local_deltas: VecDeque::new(),
            received_shadow_deltas: VecDeque::new(),
            edge_state: BTreeMap::new(),
            conflicts: VecDeque::new(),
            current_version: 0,
            retry_count: 0,
            epoch: None,
            prepared: HashSet::new(),
            in_flight: HashSet::new(),
        }
    }

    pub fn state(&self) -> ShadowState {
        self.state
    }
    pub fn config(&self) -> &ShadowDaemonConfig {
        &self.config
    }

    /// Compatibilité : Unix epoch signifie « aucun ACK ». Préférer l'Option explicite.
    pub fn last_synced(&self) -> Timestamp {
        self.last_acknowledged_at
            .clone()
            .unwrap_or(Timestamp(chrono::DateTime::UNIX_EPOCH))
    }
    pub fn last_acknowledged_at(&self) -> Option<Timestamp> {
        self.last_acknowledged_at.clone()
    }
    pub fn pending_count(&self) -> usize {
        self.pending_local_deltas.len()
    }
    pub fn conflicts(&self) -> &VecDeque<ShadowConflict> {
        &self.conflicts
    }

    /// Retrait explicite par l'appelant après présentation ou archivage des conflits.
    pub fn take_conflicts(&mut self) -> VecDeque<ShadowConflict> {
        std::mem::take(&mut self.conflicts)
    }

    pub fn get_value(&self, key: &str) -> Option<Vec<u8>> {
        self.edge_state
            .get(key)
            .filter(|delta| !delta.is_deleted)
            .map(|delta| delta.value.clone())
    }

    /// Noms des enfants immédiats ; les clés sont logiques, jamais des chemins natifs.
    pub fn list_keys(&self, prefix: &str) -> Vec<String> {
        let prefix = prefix.trim_matches('/');
        if !prefix.is_empty() && validate_key(prefix).is_err() {
            return Vec::new();
        }
        let mut names = std::collections::BTreeSet::new();
        for (key, delta) in &self.edge_state {
            if delta.is_deleted {
                continue;
            }
            let rest = if prefix.is_empty() {
                key.as_str()
            } else if let Some(rest) = key
                .strip_prefix(prefix)
                .and_then(|rest| rest.strip_prefix('/'))
            {
                rest
            } else {
                continue;
            };
            if let Some(child) = rest.split('/').next().filter(|name| !name.is_empty()) {
                names.insert(child.to_string());
            }
        }
        names.into_iter().collect()
    }

    fn check_edge_capacity(&self, delta: &ShadowDelta) -> Result<(), String> {
        let existing = self.edge_state.get(&delta.key);
        if existing.is_none() && self.edge_state.len() >= MAX_KEYS {
            return Err("Nombre maximal de clés shadow atteint".into());
        }
        let size: usize = self.edge_state.values().map(bytes).sum();
        if size - existing.map(bytes).unwrap_or(0) + bytes(delta) > MAX_SECTION_BYTES {
            return Err("État shadow saturé ; aucune mutation appliquée".into());
        }
        Ok(())
    }

    /// Fusion déterministe avec conservation bornée des contenus distants différents.
    /// Une saturation échoue avant toute mutation, y compris celle de l'horloge.
    pub fn try_apply_delta_lww(&mut self, delta: ShadowDelta) -> Result<bool, String> {
        validate_delta(&delta)?;
        let existing = self.edge_state.get(&delta.key);
        let wins = existing.is_none_or(|old| compare(&delta, old).is_gt());
        if wins {
            self.check_edge_capacity(&delta)?;
        }
        let conflict = existing
            .filter(|old| old.value != delta.value || old.is_deleted != delta.is_deleted)
            .map(|old| {
                if wins {
                    ShadowConflict {
                        retained_loser: old.clone(),
                        materialized_winner: delta.clone(),
                    }
                } else {
                    ShadowConflict {
                        retained_loser: delta.clone(),
                        materialized_winner: old.clone(),
                    }
                }
            })
            .filter(|conflict| !self.conflicts.contains(conflict));
        if let Some(conflict) = &conflict {
            let size: usize = self
                .conflicts
                .iter()
                .map(|c| bytes(&c.retained_loser) + bytes(&c.materialized_winner))
                .sum();
            if self.conflicts.len() >= MAX_SHADOW_CONFLICTS
                || size + bytes(&conflict.retained_loser) + bytes(&conflict.materialized_winner)
                    > MAX_SECTION_BYTES
            {
                return Err(
                    "Conflits shadow saturés ; archiver explicitement avant de poursuivre".into(),
                );
            }
        }
        self.current_version = self.current_version.max(delta.version);
        if let Some(conflict) = conflict {
            self.conflicts.push_back(conflict);
        }
        if wins {
            self.edge_state.insert(delta.key.clone(), delta);
        }
        Ok(wins)
    }

    /// Ancien nom conservé ; false signifie non appliqué. Pour connaître un refus
    /// de validation ou de capacité, utiliser `try_apply_delta_lww`.
    pub fn apply_delta_crdt(&mut self, delta: ShadowDelta) -> bool {
        self.try_apply_delta_lww(delta).unwrap_or(false)
    }

    /// Aucun transport TiKV/APNs/FCM n'est intégré : ne confirme jamais une connexion.
    pub fn connect(&mut self) -> Result<(), String> {
        Err(
            "Transport shadow absent : l'hôte doit réaliser et authentifier la connexion réseau"
                .into(),
        )
    }

    pub fn begin_connect(&mut self) -> Result<Uuid, String> {
        validate_transport_config(&self.config)?;
        let epoch = Uuid::new_v4();
        self.epoch = Some(epoch);
        self.prepared.clear();
        self.in_flight.clear();
        self.state = ShadowState::Connecting;
        Ok(epoch)
    }

    /// À appeler uniquement après un handshake réellement réussi par le transport hôte.
    pub fn confirm_transport_connected(&mut self, epoch: Uuid) -> Result<(), String> {
        validate_transport_config(&self.config)?;
        if self.epoch != Some(epoch) || self.state != ShadowState::Connecting {
            return Err("Confirmation de transport shadow périmée ou inattendue".into());
        }
        self.state = if self.pending_local_deltas.is_empty() {
            ShadowState::ActiveStreaming
        } else {
            ShadowState::SyncingState
        };
        self.retry_count = 0;
        Ok(())
    }

    pub fn queue_local_delta(&mut self, key: &str, value: &[u8]) -> Result<(), String> {
        self.queue_local(key, value, false)
    }
    pub fn queue_local_deletion(&mut self, key: &str) -> Result<(), String> {
        self.queue_local(key, &[], true)
    }
    fn queue_local(&mut self, key: &str, value: &[u8], is_deleted: bool) -> Result<(), String> {
        validate_config(&self.config)?;
        validate_key(key)?;
        if value.len() > MAX_SHADOW_VALUE_BYTES {
            return Err("Valeur shadow trop volumineuse".into());
        }
        let version = self
            .current_version
            .checked_add(1)
            .ok_or("Horloge de version shadow épuisée")?;
        let delta = ShadowDelta {
            key: key.into(),
            value: value.into(),
            version,
            timestamp: Timestamp::now(),
            is_deleted,
        };
        self.check_edge_capacity(&delta)?;
        let pending_bytes: usize = self
            .pending_local_deltas
            .iter()
            .map(|item| bytes(&item.delta))
            .sum();
        if self.pending_local_deltas.len() >= MAX_SHADOW_PENDING
            || pending_bytes + bytes(&delta) > MAX_SECTION_BYTES
        {
            return Err(
                "Boîte d'envoi shadow saturée ; conserver les modifications côté appelant".into(),
            );
        }
        // Une édition locale explicite remplace son état précédent, sans simuler un ACK.
        self.current_version = version;
        self.edge_state.insert(key.into(), delta.clone());
        self.pending_local_deltas.push_back(PendingShadowDelta {
            id: Uuid::new_v4(),
            delta,
        });
        if self.state == ShadowState::ActiveStreaming {
            self.state = ShadowState::SyncingState;
        }
        Ok(())
    }

    pub fn push_remote_delta(&mut self, delta: ShadowDelta) -> Result<(), String> {
        validate_delta(&delta)?;
        let wins = self
            .edge_state
            .get(&delta.key)
            .is_none_or(|old| compare(&delta, old).is_gt());
        if wins
            && (self.received_shadow_deltas.len() >= MAX_SHADOW_PENDING
                || self.received_shadow_deltas.iter().map(bytes).sum::<usize>() + bytes(&delta)
                    > MAX_SECTION_BYTES)
        {
            return Err("Journal des réceptions saturé ; le consommer avant de poursuivre".into());
        }
        if self.try_apply_delta_lww(delta.clone())? {
            self.received_shadow_deltas.push_back(delta);
        }
        Ok(())
    }
    pub fn received_deltas(&self) -> &VecDeque<ShadowDelta> {
        &self.received_shadow_deltas
    }
    pub fn take_received_deltas(&mut self) -> VecDeque<ShadowDelta> {
        std::mem::take(&mut self.received_shadow_deltas)
    }

    /// Prépare un lot sans retirer ses opérations. Les limites comptent les octets
    /// clé+valeur, pas l'encapsulation réseau. L'hôte envoie les IDs avec les deltas.
    pub fn prepare_batch(
        &mut self,
        max_items: usize,
        max_bytes: usize,
    ) -> Result<ShadowBatch, String> {
        if !matches!(
            self.state,
            ShadowState::SyncingState | ShadowState::ActiveStreaming
        ) {
            return Err("Transport shadow non confirmé ; aucun lot réseau disponible".into());
        }
        if !(1..=MAX_SHADOW_PENDING).contains(&max_items)
            || !(1..=MAX_SECTION_BYTES).contains(&max_bytes)
        {
            return Err("Limites du lot shadow invalides".into());
        }
        let mut items = Vec::new();
        let mut total = 0;
        for item in &self.pending_local_deltas {
            if items.len() == max_items || total + bytes(&item.delta) > max_bytes {
                break;
            }
            total += bytes(&item.delta);
            items.push(item.clone());
        }
        if items.is_empty() && !self.pending_local_deltas.is_empty() {
            return Err("Budget du lot inférieur à la première opération en attente".into());
        }
        self.prepared.extend(items.iter().map(|item| item.id));
        Ok(ShadowBatch {
            epoch: self.epoch.ok_or("Session shadow absente")?,
            items,
        })
    }

    /// Le transport hôte confirme l'envoi effectif ; préparer un lot ne suffit
    /// pas à rendre ses IDs acquittables. Un échec d'envoi laisse les données intactes.
    pub fn mark_batch_sent(&mut self, epoch: Uuid, ids: &[Uuid]) -> Result<(), String> {
        if self.epoch != Some(epoch)
            || !matches!(
                self.state,
                ShadowState::SyncingState | ShadowState::ActiveStreaming
            )
            || ids.is_empty()
            || ids.len() > MAX_SHADOW_PENDING
        {
            return Err("Envoi shadow sans session ou lot valide".into());
        }
        let unique: HashSet<_> = ids.iter().copied().collect();
        if unique.len() != ids.len() || ids.iter().any(|id| !self.prepared.contains(id)) {
            return Err("Envoi shadow non préparé ou répété dans le même lot".into());
        }
        self.in_flight.extend(unique);
        Ok(())
    }

    /// Seuls les IDs envoyés dans cette session peuvent être acquittés. Un ACK
    /// invalide est atomiquement refusé ; rejouer un ACK déjà consommé est refusé.
    pub fn acknowledge(&mut self, epoch: Uuid, ids: &[Uuid]) -> Result<usize, String> {
        if self.epoch != Some(epoch)
            || !matches!(
                self.state,
                ShadowState::SyncingState | ShadowState::ActiveStreaming
            )
        {
            return Err("ACK shadow d'une session périmée ou hors ligne".into());
        }
        if ids.len() > MAX_SHADOW_PENDING {
            return Err("ACK shadow trop volumineux".into());
        }
        let unique: HashSet<_> = ids.iter().copied().collect();
        if ids.is_empty()
            || unique.len() != ids.len()
            || ids.iter().any(|id| !self.in_flight.contains(id))
        {
            return Err("ACK shadow vide, répété ou non proposé au transport".into());
        }
        self.pending_local_deltas
            .retain(|item| !unique.contains(&item.id));
        self.prepared.retain(|id| !unique.contains(id));
        self.in_flight.retain(|id| !unique.contains(id));
        self.last_acknowledged_at = Some(Timestamp::now());
        self.state = if self.pending_local_deltas.is_empty() {
            ShadowState::ActiveStreaming
        } else {
            ShadowState::SyncingState
        };
        Ok(ids.len())
    }

    /// Compatibilité sans transport : zéro opération acquittée, aucune suppression.
    pub fn sync_deltas(&mut self) -> usize {
        0
    }
    fn invalidate_transport(&mut self) {
        self.epoch = None;
        self.prepared.clear();
        self.in_flight.clear();
    }
    pub fn enter_doze(&mut self) {
        self.invalidate_transport();
        self.state = ShadowState::SuspendedDozing;
    }
    pub fn wake_and_resync(&mut self) -> usize {
        self.invalidate_transport();
        self.state = ShadowState::Disconnected;
        0
    }
    pub fn handle_network_disconnect(&mut self) {
        self.invalidate_transport();
        self.retry_count = self.retry_count.saturating_add(1);
        self.state = ShadowState::ReconnectingBackoff;
    }
    /// Délai conseillé, plafonné à 32 s ; le transport hôte réalise l'attente.
    pub fn backoff_delay_ms(&self) -> u64 {
        500 * (1_u64 << self.retry_count.min(6))
    }

    /// Snapshot JSON borné ; ne fait aucune écriture disque. Les jetons push et
    /// l'état de connexion ne sont pas persistés, contrairement aux IDs et conflits.
    /// Les notifications `received_deltas` sont éphémères ; leurs contenus acceptés
    /// restent dans l'état matérialisé ou les conflits.
    pub fn snapshot_json(&self) -> Result<Vec<u8>, String> {
        validate_config(&self.config)?;
        let mut config = self.config.clone();
        config.push_token = None;
        let snapshot = ShadowSnapshot {
            schema_version: 1,
            config,
            last_acknowledged_at: self.last_acknowledged_at.clone(),
            current_version: self.current_version,
            pending: self.pending_local_deltas.clone(),
            edge_state: self.edge_state.values().cloned().collect(),
            conflicts: self.conflicts.clone(),
        };
        let bytes = serde_json::to_vec(&snapshot).map_err(|error| error.to_string())?;
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            return Err("Snapshot shadow trop volumineux".into());
        }
        Ok(bytes)
    }

    pub fn from_snapshot_json(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            return Err("Snapshot shadow trop volumineux".into());
        }
        let snapshot: ShadowSnapshot = serde_json::from_slice(bytes)
            .map_err(|error| format!("Snapshot shadow illisible : {error}"))?;
        if snapshot.schema_version != 1
            || snapshot.pending.len() > MAX_SHADOW_PENDING
            || snapshot.edge_state.len() > MAX_KEYS
            || snapshot.conflicts.len() > MAX_SHADOW_CONFLICTS
        {
            return Err("Version ou cardinalités du snapshot shadow invalides".into());
        }
        validate_config(&snapshot.config)?;
        if snapshot.config.push_token.is_some() {
            return Err("Un snapshot shadow ne doit pas contenir de jeton push".into());
        }
        let mut sync = Self::new(snapshot.config);
        sync.current_version = snapshot.current_version;
        sync.last_acknowledged_at = snapshot.last_acknowledged_at;
        for delta in snapshot.edge_state {
            validate_delta(&delta)?;
            if delta.version > sync.current_version || sync.edge_state.contains_key(&delta.key) {
                return Err("Version incohérente ou clé dupliquée dans le snapshot".into());
            }
            sync.check_edge_capacity(&delta)?;
            sync.edge_state.insert(delta.key.clone(), delta);
        }
        let mut ids = HashSet::new();
        let mut pending_bytes = 0;
        let mut previous_version = 0;
        for item in snapshot.pending {
            validate_delta(&item.delta)?;
            pending_bytes += self::bytes(&item.delta);
            if item.id.is_nil()
                || !ids.insert(item.id)
                || item.delta.version <= previous_version
                || item.delta.version > sync.current_version
                || pending_bytes > MAX_SECTION_BYTES
                || sync
                    .edge_state
                    .get(&item.delta.key)
                    .is_none_or(|current| compare(current, &item.delta).is_lt())
            {
                return Err("Opérations en attente incohérentes dans le snapshot".into());
            }
            previous_version = item.delta.version;
            sync.pending_local_deltas.push_back(item);
        }
        let mut conflict_bytes = 0;
        for conflict in snapshot.conflicts {
            validate_delta(&conflict.retained_loser)?;
            validate_delta(&conflict.materialized_winner)?;
            conflict_bytes +=
                self::bytes(&conflict.retained_loser) + self::bytes(&conflict.materialized_winner);
            if conflict.retained_loser.key != conflict.materialized_winner.key
                || !compare(&conflict.retained_loser, &conflict.materialized_winner).is_lt()
                || conflict.materialized_winner.version > sync.current_version
                || conflict_bytes > MAX_SECTION_BYTES
                || sync.conflicts.contains(&conflict)
                || sync
                    .edge_state
                    .get(&conflict.materialized_winner.key)
                    .is_none_or(|current| compare(current, &conflict.materialized_winner).is_lt())
                || (conflict.retained_loser.value == conflict.materialized_winner.value
                    && conflict.retained_loser.is_deleted
                        == conflict.materialized_winner.is_deleted)
            {
                return Err("Conflits incohérents dans le snapshot shadow".into());
            }
            sync.conflicts.push_back(conflict);
        }
        Ok(sync)
    }
}

impl Default for MobileShadowSync {
    fn default() -> Self {
        Self::new(ShadowDaemonConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delta(key: &str, value: &[u8], version: u64) -> ShadowDelta {
        ShadowDelta {
            key: key.into(),
            value: value.into(),
            version,
            timestamp: Timestamp(chrono::DateTime::UNIX_EPOCH),
            is_deleted: false,
        }
    }
    fn configured_sync() -> MobileShadowSync {
        MobileShadowSync::new(ShadowDaemonConfig {
            cluster_endpoint: "https://shadow.example.invalid".into(),
            ..ShadowDaemonConfig::default()
        })
    }

    fn session(sync: &mut MobileShadowSync) -> Uuid {
        let epoch = sync.begin_connect().unwrap();
        sync.confirm_transport_connected(epoch).unwrap();
        epoch
    }
    fn sent(sync: &mut MobileShadowSync, count: usize) -> ShadowBatch {
        let batch = sync.prepare_batch(count, MAX_SECTION_BYTES).unwrap();
        sync.mark_batch_sent(
            batch.epoch,
            &batch.items.iter().map(|item| item.id).collect::<Vec<_>>(),
        )
        .unwrap();
        batch
    }

    #[test]
    fn default_offline_identity_is_unique_and_persists_without_provider() {
        let mut first = MobileShadowSync::default();
        let second = MobileShadowSync::default();
        let id = first.config().device_id.clone();
        assert_ne!(id, second.config().device_id);
        assert!(!Uuid::parse_str(&id).unwrap().is_nil());
        assert!(first.config().cluster_endpoint.is_empty());
        assert!(
            first
                .begin_connect()
                .unwrap_err()
                .contains("Endpoint shadow absent")
        );
        assert!(first.confirm_transport_connected(Uuid::new_v4()).is_err());
        assert_eq!(first.state(), ShadowState::Disconnected);
        first
            .queue_local_delta("offline/note", b"preserved")
            .unwrap();
        let mut restored =
            MobileShadowSync::from_snapshot_json(&first.snapshot_json().unwrap()).unwrap();
        assert_eq!(restored.config().device_id, id);
        assert!(restored.config().cluster_endpoint.is_empty());
        assert_eq!(restored.pending_count(), 1);
        assert_eq!(
            restored.get_value("offline/note"),
            Some(b"preserved".to_vec())
        );
        assert!(restored.begin_connect().is_err());
    }

    #[test]
    fn transport_confirmation_requires_endpoint_and_explicit_host_event() {
        let mut sync = configured_sync();
        let epoch = sync.begin_connect().unwrap();
        assert_eq!(sync.state(), ShadowState::Connecting);
        assert!(sync.prepare_batch(1, 100).is_err());
        assert!(sync.connect().is_err());
        assert_eq!(sync.state(), ShadowState::Connecting);
        sync.config.cluster_endpoint = " ".into();
        assert!(sync.confirm_transport_connected(epoch).is_err());
        assert_eq!(sync.state(), ShadowState::Connecting);
        sync.config.cluster_endpoint = "https://shadow.example.invalid".into();
        sync.confirm_transport_connected(epoch).unwrap();
        assert_eq!(sync.state(), ShadowState::ActiveStreaming);
    }

    #[test]
    fn offline_sleep_and_wake_never_claim_connection_or_drop_edits() {
        let mut sync = MobileShadowSync::default();
        sync.queue_local_delta("notes/a", b"offline").unwrap();
        sync.queue_local_deletion("notes/b").unwrap();
        assert!(
            sync.connect()
                .unwrap_err()
                .contains("Transport shadow absent")
        );
        assert_eq!(sync.state(), ShadowState::Disconnected);
        assert!(sync.prepare_batch(1, 1024).is_err());
        assert_eq!(sync.sync_deltas(), 0);
        sync.enter_doze();
        assert_eq!(sync.state(), ShadowState::SuspendedDozing);
        assert_eq!(sync.wake_and_resync(), 0);
        assert_eq!(sync.state(), ShadowState::Disconnected);
        assert_eq!(sync.pending_count(), 2);
        assert!(sync.last_acknowledged_at().is_none());
        assert_eq!(sync.get_value("notes/a"), Some(b"offline".to_vec()));
    }

    #[test]
    fn acknowledgements_require_sent_ids_and_are_atomic_and_session_bound() {
        let mut sync = configured_sync();
        sync.queue_local_delta("a", b"first").unwrap();
        sync.queue_local_delta("b", b"second").unwrap();
        let epoch = session(&mut sync);
        let prepared = sync.prepare_batch(1, 100).unwrap();
        let first = prepared.items[0].id;
        let unsent = sync.pending_local_deltas[1].id;
        assert!(sync.acknowledge(epoch, &[first]).is_err());
        assert!(sync.mark_batch_sent(epoch, &[unsent]).is_err());
        sync.mark_batch_sent(epoch, &[first]).unwrap();
        assert!(sync.acknowledge(epoch, &[first, unsent]).is_err());
        assert!(sync.acknowledge(epoch, &[first, first]).is_err());
        assert!(sync.acknowledge(Uuid::new_v4(), &[first]).is_err());
        assert_eq!(sync.pending_count(), 2);
        assert!(sync.last_acknowledged_at().is_none());
        sync.queue_local_delta("a", b"newer").unwrap();
        assert_eq!(sync.acknowledge(epoch, &[first]).unwrap(), 1);
        assert_eq!(sync.pending_count(), 2);
        assert_eq!(sync.get_value("a"), Some(b"newer".to_vec()));
        assert!(sync.last_acknowledged_at().is_some());
        assert!(sync.acknowledge(epoch, &[first]).is_err());
        let batch = sent(&mut sync, 2);
        sync.handle_network_disconnect();
        assert!(sync.acknowledge(epoch, &[batch.items[0].id]).is_err());
        let next = session(&mut sync);
        assert_ne!(next, epoch);
        assert!(sync.acknowledge(next, &[batch.items[0].id]).is_err());
        let resent = sent(&mut sync, 2);
        assert_eq!(resent.items, batch.items);
        assert_eq!(
            sync.acknowledge(
                next,
                &resent.items.iter().map(|item| item.id).collect::<Vec<_>>()
            )
            .unwrap(),
            2
        );
        assert_eq!(sync.state(), ShadowState::ActiveStreaming);
    }

    #[test]
    fn doze_invalidates_handshake_and_inflight_acknowledgements() {
        let mut sync = configured_sync();
        let pending = sync.begin_connect().unwrap();
        sync.enter_doze();
        assert!(sync.confirm_transport_connected(pending).is_err());
        sync.queue_local_delta("a", b"value").unwrap();
        let epoch = session(&mut sync);
        let batch = sent(&mut sync, 1);
        sync.enter_doze();
        assert!(sync.acknowledge(epoch, &[batch.items[0].id]).is_err());
        sync.wake_and_resync();
        assert_eq!(sync.pending_count(), 1);
    }

    #[test]
    fn snapshot_restores_pending_ids_conflicts_and_rejects_old_sessions() {
        let mut sync = configured_sync();
        sync.queue_local_delta("a", b"local").unwrap();
        sync.push_remote_delta(delta("a", b"remote", 9)).unwrap();
        let epoch = session(&mut sync);
        let batch = sent(&mut sync, 1);
        sync.config.push_token = Some("sensitive-push-token".into());
        let bytes = sync.snapshot_json().unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("sensitive-push-token"));
        let mut restored = MobileShadowSync::from_snapshot_json(&bytes).unwrap();
        assert_eq!(restored.state(), ShadowState::Disconnected);
        assert_eq!(restored.conflicts(), sync.conflicts());
        assert_eq!(restored.get_value("a"), Some(b"remote".to_vec()));
        assert!(restored.acknowledge(epoch, &[batch.items[0].id]).is_err());
        let fresh = session(&mut restored);
        assert_ne!(fresh, epoch);
        assert_eq!(sent(&mut restored, 1).items, batch.items);
        restored.acknowledge(fresh, &[batch.items[0].id]).unwrap();
        assert_eq!(restored.pending_count(), 0);
    }

    #[test]
    fn tied_values_converge_and_losing_content_remains_observable() {
        let low = delta("a", b"alpha", 4);
        let high = delta("a", b"zulu", 4);
        let mut first = MobileShadowSync::default();
        let mut second = MobileShadowSync::default();
        first.push_remote_delta(low.clone()).unwrap();
        first.push_remote_delta(high.clone()).unwrap();
        second.push_remote_delta(high.clone()).unwrap();
        second.push_remote_delta(low.clone()).unwrap();
        assert_eq!(first.get_value("a"), second.get_value("a"));
        assert_eq!(first.get_value("a"), Some(b"zulu".to_vec()));
        assert_eq!(first.conflicts(), second.conflicts());
        second.push_remote_delta(low).unwrap();
        assert_eq!(second.conflicts().len(), 1); // Rejeu sans duplication.
        assert_eq!(second.received_deltas().len(), 1);
        let mut tombstone = high;
        tombstone.is_deleted = true;
        tombstone.value.clear();
        first.push_remote_delta(tombstone).unwrap();
        assert_eq!(first.get_value("a"), None);
        assert_eq!(first.conflicts().len(), 2);
        assert_eq!(
            first.conflicts().back().unwrap().retained_loser.value,
            b"zulu"
        );
    }

    #[test]
    fn rejected_keys_limits_and_version_overflow_leave_state_unchanged() {
        let mut sync = MobileShadowSync::default();
        let initial = sync.snapshot_json().unwrap();
        for key in [
            "", "/a", "../a", "a/../b", "a//b", "a/", "C:/a", "a\\b", "a\0b",
        ] {
            assert!(sync.queue_local_delta(key, b"x").is_err(), "{key:?}");
            assert!(sync.push_remote_delta(delta(key, b"x", 9)).is_err());
        }
        assert!(
            sync.queue_local_delta("large", &vec![0; MAX_SHADOW_VALUE_BYTES + 1])
                .is_err()
        );
        assert_eq!(sync.snapshot_json().unwrap(), initial);
        sync.push_remote_delta(delta("a", b"max", u64::MAX))
            .unwrap();
        let before = sync.snapshot_json().unwrap();
        assert!(sync.queue_local_delta("a", b"overflow").is_err());
        assert_eq!(sync.snapshot_json().unwrap(), before);
    }

    #[test]
    fn full_outbox_and_conflict_log_refuse_mutations_without_losing_data() {
        let mut sync = MobileShadowSync::default();
        for _ in 0..MAX_SHADOW_PENDING {
            sync.queue_local_delta("a", b"offline").unwrap();
        }
        let before = sync.snapshot_json().unwrap();
        assert!(sync.queue_local_deletion("a").is_err());
        assert_eq!(sync.snapshot_json().unwrap(), before);
        let mut sync = MobileShadowSync::default();
        sync.push_remote_delta(delta("a", b"winner", 1000)).unwrap();
        for version in 1..=MAX_SHADOW_CONFLICTS as u64 {
            sync.push_remote_delta(delta("a", b"loser", version))
                .unwrap();
        }
        let before = sync.snapshot_json().unwrap();
        assert!(sync.push_remote_delta(delta("a", b"another", 999)).is_err());
        assert_eq!(sync.snapshot_json().unwrap(), before);
        assert_eq!(sync.take_conflicts().len(), MAX_SHADOW_CONFLICTS);
        sync.push_remote_delta(delta("a", b"another", 999)).unwrap();
    }

    #[test]
    fn reception_and_byte_limits_require_explicit_consumption() {
        let mut sync = MobileShadowSync::default();
        for version in 1..=MAX_SHADOW_PENDING as u64 {
            sync.push_remote_delta(delta("a", b"same content", version))
                .unwrap();
        }
        let before = sync.snapshot_json().unwrap();
        assert!(
            sync.push_remote_delta(delta("a", b"same content", 999))
                .is_err()
        );
        assert_eq!(sync.snapshot_json().unwrap(), before);
        assert_eq!(sync.take_received_deltas().len(), MAX_SHADOW_PENDING);
        sync.push_remote_delta(delta("a", b"same content", 999))
            .unwrap();
        let mut sync = MobileShadowSync::default();
        let payload = vec![0; MAX_SHADOW_VALUE_BYTES];
        for _ in 0..15 {
            sync.queue_local_delta("a", &payload).unwrap();
        }
        let version = sync.current_version;
        assert!(sync.queue_local_delta("a", &payload).is_err());
        assert_eq!(sync.current_version, version);
        assert_eq!(sync.pending_count(), 15);
    }

    #[test]
    fn malformed_snapshots_cannot_bypass_validation_or_replay_ids() {
        let mut sync = MobileShadowSync::default();
        sync.queue_local_delta("a", b"one").unwrap();
        sync.queue_local_delta("b", b"two").unwrap();
        let original: serde_json::Value =
            serde_json::from_slice(&sync.snapshot_json().unwrap()).unwrap();
        let mut mutations = Vec::new();
        let mut changed = original.clone();
        changed["schema_version"] = 99.into();
        mutations.push(changed);
        let mut changed = original.clone();
        changed["edge_state"][0]["key"] = "../escape".into();
        mutations.push(changed);
        let mut changed = original.clone();
        changed["pending"][1]["id"] = changed["pending"][0]["id"].clone();
        mutations.push(changed);
        let mut changed = original.clone();
        changed["current_version"] = 0.into();
        mutations.push(changed);
        let mut changed = original.clone();
        changed["pending"][0]["delta"]["value"] = serde_json::json!([255]);
        mutations.push(changed);
        let mut changed = original.clone();
        changed["edge_state"][0]["is_deleted"] = true.into();
        mutations.push(changed);
        let mut changed = original.clone();
        changed["config"]["push_token"] = "do-not-persist".into();
        mutations.push(changed);
        let mut changed = original;
        changed["surprise"] = true.into();
        mutations.push(changed);
        for changed in mutations {
            assert!(
                MobileShadowSync::from_snapshot_json(&serde_json::to_vec(&changed).unwrap())
                    .is_err(),
                "{changed}"
            );
        }
        assert!(MobileShadowSync::from_snapshot_json(b"{truncated").is_err());
    }

    #[test]
    fn batch_limits_and_backoff_are_bounded_without_consuming_pending() {
        let mut sync = configured_sync();
        sync.queue_local_delta("a", b"1234").unwrap();
        sync.queue_local_delta("b", b"5678").unwrap();
        session(&mut sync);
        assert!(sync.prepare_batch(0, 100).is_err());
        assert!(sync.prepare_batch(1, 4).is_err());
        assert_eq!(sync.prepare_batch(2, 5).unwrap().items.len(), 1);
        assert_eq!(sync.pending_count(), 2);
        sync.retry_count = u32::MAX;
        sync.handle_network_disconnect();
        assert_eq!(sync.backoff_delay_ms(), 32_000);
        assert_eq!(sync.pending_count(), 2);
    }
}
