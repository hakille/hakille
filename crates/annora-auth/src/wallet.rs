use annora_core::{AnnoraError, CapabilityScope, Result, SpendingCategory, Timestamp, UserId, WalletCapability};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendRecord {
    pub amount: u64,
    pub timestamp: Timestamp,
    pub description: String,
    pub category: SpendingCategory,
}

#[derive(Default)]
pub struct WalletManager {
    spent: HashMap<WalletCapability, u64>,
    records: Vec<SpendRecord>,
}

impl WalletManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_wallet_cap(
        &self,
        _user_id: UserId,
        max_amount: u64,
        currency: String,
        category: SpendingCategory,
        expires: Timestamp,
    ) -> WalletCapability {
        WalletCapability {
            scope: CapabilityScope::Wallet,
            max_amount_cents: max_amount,
            currency,
            category,
            expires_at: expires,
        }
    }

    pub fn check_budget(&self, wallet: &WalletCapability, amount: u64) -> Result<()> {
        let current_spent = self.spent.get(wallet).copied().unwrap_or(0);
        if current_spent + amount > wallet.max_amount_cents {
            return Err(AnnoraError::CapabilityDenied(format!(
                "Wallet budget exceeded: requested {}, current spent {}, limit {}",
                amount, current_spent, wallet.max_amount_cents
            )));
        }
        Ok(())
    }

    pub fn record_spend(
        &mut self,
        wallet: &mut WalletCapability,
        amount: u64,
        description: String,
    ) -> Result<()> {
        self.check_budget(wallet, amount)?;
        let current_spent = self.spent.entry(wallet.clone()).or_insert(0);
        *current_spent += amount;
        self.records.push(SpendRecord {
            amount,
            timestamp: Timestamp::now(),
            description,
            category: wallet.category.clone(),
        });
        Ok(())
    }

    pub fn remaining_budget(&self, wallet: &WalletCapability) -> u64 {
        let current_spent = self.spent.get(wallet).copied().unwrap_or(0);
        wallet.max_amount_cents.saturating_sub(current_spent)
    }

    pub fn history(&self) -> &[SpendRecord] {
        &self.records
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_wallet_budget_and_spend() {
        let mut manager = WalletManager::new();
        let user_id = UserId(Uuid::now_v7());
        let mut wallet = manager.create_wallet_cap(
            user_id,
            5000, // 50.00 EUR
            "EUR".into(),
            SpendingCategory::Food,
            Timestamp::now(),
        );

        assert_eq!(manager.remaining_budget(&wallet), 5000);
        assert!(manager.check_budget(&wallet, 3000).is_ok());

        // Spend 30.00 EUR
        manager
            .record_spend(&mut wallet, 3000, "Lunch".into())
            .expect("spend should succeed");

        assert_eq!(manager.remaining_budget(&wallet), 2000);
        assert_eq!(manager.history().len(), 1);

        // Try to spend 25.00 EUR (exceeds remaining 20.00 EUR)
        assert!(manager.check_budget(&wallet, 2500).is_err());
        assert!(manager
            .record_spend(&mut wallet, 2500, "Dinner".into())
            .is_err());

        // Exact remaining spend of 20.00 EUR
        manager
            .record_spend(&mut wallet, 2000, "Snacks".into())
            .expect("exact budget spend should succeed");
        assert_eq!(manager.remaining_budget(&wallet), 0);

        // Any further spend should fail
        assert!(manager.record_spend(&mut wallet, 1, "Gum".into()).is_err());
    }
}

