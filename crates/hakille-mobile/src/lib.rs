//! Cœur mobile partagé Android/iOS : protocole complet et états bornés.
//! Aucun transport, credential, capture audio ou modèle neuronal n'est embarqué.
//! L'hôte natif assure le réseau et la persistance préalable du checkpoint.
mod core;
mod persistence;
pub mod types;

use core::{Core, MAX_JSON_BYTES};
use persistence::MAX_CHECKPOINT_BYTES;
use std::{sync::Mutex, time::Instant};
pub use types::{BridgeResult, MobileAction, MobileSnapshot};
use types::{GatewayResponse, SavedDraft};

#[cfg(feature = "ffi")]
uniffi::setup_scaffolding!();

#[cfg_attr(feature = "ffi", derive(uniffi::Error))]
#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum MobileError {
    #[error("{message}")]
    OperationFailed { message: String },
}

impl From<String> for MobileError {
    fn from(message: String) -> Self {
        Self::OperationFailed { message }
    }
}

/// Une instance par session d'application. Aucun appel n'effectue d'accès
/// réseau. Le résultat `request` doit être envoyé explicitement par l'hôte.
#[cfg_attr(feature = "ffi", derive(uniffi::Object))]
pub struct MobileBridge {
    state: Mutex<Core>,
}

impl Default for MobileBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(feature = "ffi", uniffi::export)]
impl MobileBridge {
    #[cfg_attr(feature = "ffi", uniffi::constructor)]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(Core::default()),
        }
    }

    pub fn snapshot_json(&self) -> String {
        match self.state.lock() {
            Ok(state) => serde_json::to_string(&state.snapshot).unwrap_or_else(|_| "null".into()),
            Err(_) => "null".into(),
        }
    }

    /// Actions JSON taguées `type` ; réponse {ok,error,request,snapshot}.
    pub fn dispatch_json(&self, action_json: String) -> String {
        let Ok(mut state) = self.state.lock() else {
            return unavailable();
        };
        let action = if action_json.len() > MAX_JSON_BYTES {
            Err("Action JSON supérieure à 1 Mio".to_owned())
        } else {
            serde_json::from_str::<MobileAction>(&action_json)
                .map_err(|_| "Action mobile inconnue ou JSON invalide".into())
        };
        let action = match action {
            Ok(action) => action,
            Err(error) => return result(&state, Err(error)),
        };
        let before = state.clone();
        let outcome = state.dispatch(action, Instant::now());
        if outcome.is_ok()
            && let Err(error) = state.trim_history_to_budget()
        {
            *state = before;
            return result(&state, Err(error));
        }
        result(&state, outcome)
    }

    /// Reçoit une enveloppe gateway {session_id,frames,error?}, avec l'ID
    /// conservé par l'appelant. Tous les suivis du batch sont appliqués.
    pub fn receive_json(&self, request_id: u64, response_json: String) -> String {
        let Ok(mut state) = self.state.lock() else {
            return unavailable();
        };
        let parsed = if response_json.len() > MAX_JSON_BYTES {
            Err("Réponse gateway supérieure à 1 Mio".to_owned())
        } else {
            serde_json::from_str::<GatewayResponse>(&response_json)
                .map_err(|_| "Enveloppe gateway JSON invalide".into())
        };
        let outcome = match parsed {
            Ok(reply) => state.receive(request_id, reply),
            Err(error) => Err(error),
        };
        if let Err(error) = &outcome {
            // Une réponse illisible après émission n'autorise jamais à
            // rejouer une mutation. Un ID ancien ne touche pas le courant.
            if state
                .snapshot
                .pending
                .as_ref()
                .is_some_and(|p| p.request_id == request_id)
            {
                let _ = state.fail(request_id, error);
            }
        }
        result(&state, outcome.map(|_| None))
    }

    /// Une erreur réseau ne dit pas si le hub a déjà appliqué la mutation.
    /// L'hôte ne doit transmettre ici ni URL secrète ni token d'authentification.
    pub fn fail_request(&self, request_id: u64, message: String) -> String {
        let Ok(mut state) = self.state.lock() else {
            return unavailable();
        };
        let outcome = state.fail(request_id, &message).map(|_| None);
        result(&state, outcome)
    }

    /// Checkpoint v2 : brouillon, compteur et marqueurs d'opérations sans
    /// payload, hub, token ou carte. À publier durablement AVANT l'émission
    /// d'une trame retournée par dispatch_json. Aucun rejeu au chargement.
    pub fn draft_json(&self) -> String {
        match self.state.lock() {
            Ok(state) => {
                serde_json::to_string(&state.saved_draft()).unwrap_or_else(|_| "null".into())
            }
            Err(_) => "null".into(),
        }
    }

    pub fn restore_draft_json(&self, draft_json: String) -> String {
        let Ok(mut state) = self.state.lock() else {
            return unavailable();
        };
        if state.snapshot.pending.is_some() {
            return result(
                &state,
                Err("Restauration refusée pendant une requête".into()),
            );
        }
        let parsed = if draft_json.len() > MAX_CHECKPOINT_BYTES {
            Err("Brouillon persistant trop volumineux".to_owned())
        } else {
            serde_json::from_str::<SavedDraft>(&draft_json)
                .map_err(|_| "Brouillon persistant invalide".into())
        };
        let outcome = parsed.and_then(|draft| state.restore_saved(draft).map(|_| None));
        result(&state, outcome)
    }

    pub fn battery_tier(&self, battery_percent: f32, charging: bool) -> String {
        let engine =
            hakille_substrate_mobile::BatteryConservationEngine::new(battery_percent, charging);
        match engine.current_tier() {
            hakille_substrate_mobile::BatteryTier::Normal => "normal".into(),
            hakille_substrate_mobile::BatteryTier::Conserve => "conserve".into(),
            hakille_substrate_mobile::BatteryTier::Critical => "critical".into(),
            hakille_substrate_mobile::BatteryTier::Charging => "charging".into(),
            hakille_substrate_mobile::BatteryTier::Unknown => "unknown".into(),
        }
    }

    pub fn should_offload_inference(
        &self,
        battery_percent: f32,
        charging: bool,
        network_available: bool,
    ) -> bool {
        let engine =
            hakille_substrate_mobile::BatteryConservationEngine::new(battery_percent, charging);
        engine.should_offload_inference_with_network(network_available)
    }

    pub fn perception_fps_limit(&self, battery_percent: f32, charging: bool) -> u32 {
        let engine =
            hakille_substrate_mobile::BatteryConservationEngine::new(battery_percent, charging);
        engine.perception_fps()
    }
}

#[cfg_attr(feature = "ffi", derive(uniffi::Object))]
pub struct MobileBatteryManager {
    engine: Mutex<hakille_substrate_mobile::BatteryConservationEngine>,
}

#[cfg_attr(feature = "ffi", uniffi::export)]
impl MobileBatteryManager {
    #[cfg_attr(feature = "ffi", uniffi::constructor)]
    pub fn new(battery_percent: f32, charging: bool) -> Self {
        Self {
            engine: Mutex::new(hakille_substrate_mobile::BatteryConservationEngine::new(
                battery_percent,
                charging,
            )),
        }
    }

    pub fn update(&self, battery_percent: f32, charging: bool) -> Result<(), MobileError> {
        let mut engine = self
            .engine
            .lock()
            .map_err(|e| MobileError::from(e.to_string()))?;
        engine
            .update_power_state(&hakille_substrate::PowerState {
                on_battery: !charging,
                battery_percent,
                charging,
            })
            .map_err(|e| MobileError::from(e.to_string()))
    }

    pub fn tier(&self) -> String {
        let Ok(engine) = self.engine.lock() else {
            return "unknown".into();
        };
        match engine.current_tier() {
            hakille_substrate_mobile::BatteryTier::Normal => "normal".into(),
            hakille_substrate_mobile::BatteryTier::Conserve => "conserve".into(),
            hakille_substrate_mobile::BatteryTier::Critical => "critical".into(),
            hakille_substrate_mobile::BatteryTier::Charging => "charging".into(),
            hakille_substrate_mobile::BatteryTier::Unknown => "unknown".into(),
        }
    }

    pub fn perception_fps(&self) -> u32 {
        self.engine.lock().map(|e| e.perception_fps()).unwrap_or(0)
    }

    pub fn should_offload(&self, network_available: bool) -> bool {
        self.engine
            .lock()
            .map(|e| e.should_offload_inference_with_network(network_available))
            .unwrap_or(false)
    }

    pub fn record_energy(&self, agent: String, micro_watt_hours: u64) -> Result<(), MobileError> {
        let engine = self
            .engine
            .lock()
            .map_err(|e| MobileError::from(e.to_string()))?;
        engine
            .record_agent_energy(&agent, micro_watt_hours)
            .map_err(|e| MobileError::from(e.to_string()))
    }

    pub fn get_energy(&self, agent: String) -> u64 {
        self.engine
            .lock()
            .map(|e| e.get_agent_energy(&agent).unwrap_or(0))
            .unwrap_or(0)
    }
}

#[cfg_attr(feature = "ffi", derive(uniffi::Object))]
pub struct MobileShadowSyncBridge {
    sync: Mutex<hakille_substrate_mobile::MobileShadowSync>,
}

#[cfg_attr(feature = "ffi", uniffi::export)]
impl MobileShadowSyncBridge {
    #[cfg_attr(feature = "ffi", uniffi::constructor)]
    pub fn new(cluster_endpoint: String, device_id: String) -> Self {
        Self {
            sync: Mutex::new(hakille_substrate_mobile::MobileShadowSync::new(
                hakille_substrate_mobile::ShadowDaemonConfig {
                    cluster_endpoint,
                    device_id,
                    ..Default::default()
                },
            )),
        }
    }

    pub fn state(&self) -> String {
        let Ok(sync) = self.sync.lock() else {
            return "unknown".into();
        };
        match sync.state() {
            hakille_substrate_mobile::ShadowState::Disconnected => "disconnected".into(),
            hakille_substrate_mobile::ShadowState::Connecting => "connecting".into(),
            hakille_substrate_mobile::ShadowState::SyncingState => "syncing_state".into(),
            hakille_substrate_mobile::ShadowState::ActiveStreaming => "active_streaming".into(),
            hakille_substrate_mobile::ShadowState::SuspendedDozing => "suspended_dozing".into(),
            hakille_substrate_mobile::ShadowState::ReconnectingBackoff => {
                "reconnecting_backoff".into()
            }
        }
    }

    pub fn pending_count(&self) -> u32 {
        self.sync
            .lock()
            .map(|s| s.pending_count() as u32)
            .unwrap_or(0)
    }

    pub fn queue_delta(&self, key: String, value: Vec<u8>) -> Result<(), MobileError> {
        let mut sync = self
            .sync
            .lock()
            .map_err(|e| MobileError::from(e.to_string()))?;
        sync.queue_local_delta(&key, &value)
            .map_err(|e| MobileError::from(e.to_string()))
    }

    pub fn queue_deletion(&self, key: String) -> Result<(), MobileError> {
        let mut sync = self
            .sync
            .lock()
            .map_err(|e| MobileError::from(e.to_string()))?;
        sync.queue_local_deletion(&key)
            .map_err(|e| MobileError::from(e.to_string()))
    }

    pub fn enter_doze(&self) {
        if let Ok(mut sync) = self.sync.lock() {
            sync.enter_doze();
        }
    }

    pub fn wake_and_resync(&self) -> u32 {
        self.sync
            .lock()
            .map(|mut s| s.wake_and_resync() as u32)
            .unwrap_or(0)
    }
}

fn result(state: &Core, outcome: Result<Option<hakille_ipc::Frame>, String>) -> String {
    let (ok, error, request) = match outcome {
        Ok(request) => (true, None, request),
        Err(error) => (false, Some(error), None),
    };
    serde_json::to_string(&BridgeResult {
        ok,
        error,
        request,
        snapshot: state.snapshot.clone(),
    })
    .unwrap_or_else(|_| unavailable())
}
fn unavailable() -> String {
    "{\"ok\":false,\"error\":\"État mobile indisponible\",\"request\":null,\"snapshot\":null}"
        .into()
}

#[cfg(test)]
mod tests;
