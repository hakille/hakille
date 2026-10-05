use annora_core::{CapabilityAction, CapabilityScope};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct BalanceStatement {
    pub account_id: String,
    pub currency: String,
    pub balance_cents: u64,
}

fn main() {
    println!("Annora Wallet (Read-Only) Agent initialized. Transaction execution is permanently forbidden.");
}

pub fn check_wallet_capabilities() -> Vec<(CapabilityScope, CapabilityAction)> {
    // Only Read action is permitted for Wallet scope!
    vec![(CapabilityScope::Wallet, CapabilityAction::Read)]
}
