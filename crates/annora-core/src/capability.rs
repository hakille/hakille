use crate::error::{AnnoraError, Result};
use crate::types::Timestamp;
use biscuit_auth::{Biscuit, KeyPair};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CapabilityScope {
    Filesystem,
    Network,
    GuiInput,
    Wallet,
    Sensor,
    ModelInference,
    Audio,
    Camera,
    FullSystem,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CapabilityAction {
    Read,
    Write,
    Execute,
    Delete,
    Admin,
}

#[derive(Debug, Clone)]
pub struct CapabilityToken {
    pub biscuit: Biscuit,
}

pub struct CapabilityAuthority {
    pub root_keypair: KeyPair,
}

impl CapabilityAuthority {
    pub fn new() -> Self {
        Self {
            root_keypair: KeyPair::new(),
        }
    }

    pub fn create_root_token(
        &self,
        scopes: Vec<CapabilityScope>,
        actions: Vec<CapabilityAction>,
        expiry: Timestamp,
    ) -> Result<CapabilityToken> {
        let mut builder = Biscuit::builder();
        for scope in &scopes {
            let s = format!("{:?}", scope).to_lowercase();
            for action in &actions {
                let a = format!("{:?}", action).to_lowercase();
                let fact = format!("right(\"{}\", \"{}\")", s, a);
                builder.add_fact(fact.as_str()).map_err(|e| AnnoraError::CapabilityDenied(e.to_string()))?;
            }
        }
        let exp_ts = expiry.0.timestamp();
        let check = format!("check if time($time), $time <= {}", exp_ts);
        builder.add_check(check.as_str()).map_err(|e| AnnoraError::CapabilityDenied(e.to_string()))?;
        
        let biscuit = builder.build(&self.root_keypair).map_err(|e| AnnoraError::CapabilityDenied(e.to_string()))?;
        Ok(CapabilityToken { biscuit })
    }

    pub fn attenuate(
        &self,
        parent_token: &CapabilityToken,
        restricted_scopes: Vec<CapabilityScope>,
        restricted_actions: Vec<CapabilityAction>,
        new_expiry: Timestamp,
    ) -> Result<CapabilityToken> {
        let mut block = biscuit_auth::builder::BlockBuilder::new();
        if restricted_scopes.is_empty() || restricted_actions.is_empty() {
            block.add_check("check if false").map_err(|_e| AnnoraError::AttenuationViolation)?;
        } else {
            for scope in &restricted_scopes {
                let s = format!("{:?}", scope).to_lowercase();
                let fact = format!("allowed_scope(\"{}\")", s);
                block.add_fact(fact.as_str()).map_err(|_e| AnnoraError::AttenuationViolation)?;
            }
            for action in &restricted_actions {
                let a = format!("{:?}", action).to_lowercase();
                let fact = format!("allowed_action(\"{}\")", a);
                block.add_fact(fact.as_str()).map_err(|_e| AnnoraError::AttenuationViolation)?;
            }
            block.add_check("check if operation($s, $a), allowed_scope($s), allowed_action($a)")
                .map_err(|_e| AnnoraError::AttenuationViolation)?;
        }
        let exp_ts = new_expiry.0.timestamp();
        let time_check = format!("check if time($time), $time <= {}", exp_ts);
        block.add_check(time_check.as_str()).map_err(|_e| AnnoraError::AttenuationViolation)?;

        let attenuated = parent_token.biscuit.append(block).map_err(|_e| AnnoraError::AttenuationViolation)?;
        Ok(CapabilityToken { biscuit: attenuated })
    }

    pub fn verify(
        &self,
        token: &CapabilityToken,
        required_scope: CapabilityScope,
        required_action: CapabilityAction,
    ) -> Result<()> {
        let mut authorizer = token.biscuit.authorizer().map_err(|e| AnnoraError::CapabilityDenied(e.to_string()))?;
        let s = format!("{:?}", required_scope).to_lowercase();
        let a = format!("{:?}", required_action).to_lowercase();
        let now_ts = chrono::Utc::now().timestamp();
        
        let time_fact = format!("time({})", now_ts);
        authorizer.add_fact(time_fact.as_str()).map_err(|e| AnnoraError::CapabilityDenied(e.to_string()))?;
        
        let op_fact = format!("operation(\"{}\", \"{}\")", s, a);
        authorizer.add_fact(op_fact.as_str()).map_err(|e| AnnoraError::CapabilityDenied(e.to_string()))?;
        let check = format!("check if right(\"{}\", \"{}\")", s, a);
        authorizer.add_check(check.as_str()).map_err(|e| AnnoraError::CapabilityDenied(e.to_string()))?;
        authorizer.allow().map_err(|e| AnnoraError::CapabilityDenied(e.to_string()))?;

        let limits = biscuit_auth::AuthorizerLimits {
            max_time: std::time::Duration::from_millis(500),
            ..Default::default()
        };
        authorizer.authorize_with_limits(limits).map_err(|e| AnnoraError::CapabilityDenied(e.to_string()))?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SpendingCategory {
    Food,
    Transport,
    Entertainment,
    Shopping,
    Bills,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct WalletCapability {
    pub scope: CapabilityScope, // Should be CapabilityScope::Wallet
    pub max_amount_cents: u64,
    pub currency: String,
    pub category: SpendingCategory,
    pub expires_at: Timestamp,
}
