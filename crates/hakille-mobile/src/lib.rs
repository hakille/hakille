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
