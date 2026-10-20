use hakille_core::{
    CapabilityScope, HakilleError, Result, SpendingCategory, Timestamp, UserId, WalletCapability,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendRecord {
    pub amount: u64,
    pub timestamp: Timestamp,
    pub description: String,
    pub category: SpendingCategory,
    pub currency: String,
}

#[derive(Default)]
pub struct WalletManager {
    spent: HashMap<WalletCapability, u64>,
    records: Vec<SpendRecord>,
}

/// Snapshot JSON pour `save/load` : `HashMap<WalletCapability, u64>` n'est
/// pas directement sérialisable en JSON (clé non-string), d'où le `Vec`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WalletSnapshot {
    spent: Vec<(WalletCapability, u64)>,
    records: Vec<SpendRecord>,
}

impl WalletManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_wallet_cap(
        &self,
        user_id: UserId,
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
            owner: Some(user_id),
        }
    }

    /// Contrôles structurels d'un cap : scope Wallet, devise renseignée,
    /// non expiré. Appelé par `check_budget`/`check_budget_for`.
    fn check_cap_validity(&self, wallet: &WalletCapability) -> Result<()> {
        if wallet.scope != CapabilityScope::Wallet {
            return Err(HakilleError::CapabilityDenied(
                "Cap inattendu : scope != Wallet".to_string(),
            ));
        }
        if wallet.currency.trim().is_empty() {
            return Err(HakilleError::CapabilityDenied(
                "Cap Wallet sans devise (currency vide)".to_string(),
            ));
        }
        if chrono::Utc::now() > wallet.expires_at.0 {
            return Err(HakilleError::CapabilityExpired);
        }
        Ok(())
    }

    pub fn check_budget(&self, wallet: &WalletCapability, amount: u64) -> Result<()> {
        self.check_cap_validity(wallet)?;
        let current_spent = self.spent.get(wallet).copied().unwrap_or(0);
        if current_spent + amount > wallet.max_amount_cents {
            return Err(HakilleError::CapabilityDenied(format!(
                "Wallet budget exceeded: requested {}, current spent {}, limit {}",
                amount, current_spent, wallet.max_amount_cents
            )));
        }
        Ok(())
    }

    /// `check_budget` + vérification devise/catégorie de la dépense
    /// contre le cap (refus si mismatch).
    pub fn check_budget_for(
        &self,
        wallet: &WalletCapability,
        amount: u64,
        currency: &str,
        category: &SpendingCategory,
    ) -> Result<()> {
        self.check_budget(wallet, amount)?;
        if wallet.currency != currency {
            return Err(HakilleError::CapabilityDenied(format!(
                "Devise incompatible : dépense en {} mais cap en {}",
                currency, wallet.currency
            )));
        }
        if &wallet.category != category {
            return Err(HakilleError::CapabilityDenied(format!(
                "Catégorie incompatible : dépense {:?} mais cap {:?}",
                category, wallet.category
            )));
        }
        Ok(())
    }

    pub fn record_spend(
        &mut self,
        wallet: &mut WalletCapability,
        amount: u64,
        description: String,
        currency: &str,
        category: SpendingCategory,
    ) -> Result<()> {
        self.check_budget_for(wallet, amount, currency, &category)?;
        let current_spent = self.spent.entry(wallet.clone()).or_insert(0);
        *current_spent += amount;
        self.records.push(SpendRecord {
            amount,
            timestamp: Timestamp::now(),
            description,
            category,
            currency: currency.to_string(),
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

    /// Total dépensé aujourd'hui (UTC) tous caps confondus.
    pub fn daily_spent_cents(&self) -> u64 {
        let today = chrono::Utc::now().date_naive();
        self.records
            .iter()
            .filter(|r| r.timestamp.0.date_naive() == today)
            .map(|r| r.amount)
            .sum()
    }

    /// Plafond journalier global : refuse si `déjà_dépensé_aujourd'hui + amount > daily_limit_cents`.
    pub fn check_daily_cap(&self, amount: u64, daily_limit_cents: u64) -> Result<()> {
        let spent_today = self.daily_spent_cents();
        if spent_today.saturating_add(amount) > daily_limit_cents {
            return Err(HakilleError::CapabilityDenied(format!(
                "Plafond journalier dépassé : déjà {}c + {}c > limite {}c",
                spent_today, amount, daily_limit_cents
            )));
        }
        Ok(())
    }

    /// Persiste `spent` + `records` en JSON (snapshot).
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let snap = WalletSnapshot {
            spent: self.spent.iter().map(|(k, v)| (k.clone(), *v)).collect(),
            records: self.records.clone(),
        };
        let json = serde_json::to_vec_pretty(&snap)
            .map_err(|e| HakilleError::SerializationError(e.to_string()))?;
        std::fs::write(path.as_ref(), json)
            .map_err(|e| HakilleError::StorageError(e.to_string()))?;
        Ok(())
    }

    /// Recharge un snapshot persisté via `save_to_file`.
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self> {
        let bytes =
            std::fs::read(path.as_ref()).map_err(|e| HakilleError::StorageError(e.to_string()))?;
        let snap: WalletSnapshot = serde_json::from_slice(&bytes)
            .map_err(|e| HakilleError::SerializationError(e.to_string()))?;
        Ok(Self {
            spent: snap.spent.into_iter().collect(),
            records: snap.records,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn future_ts() -> Timestamp {
        Timestamp(chrono::Utc::now() + chrono::Duration::seconds(3600))
    }

    #[test]
    fn test_wallet_budget_and_spend() {
        let mut manager = WalletManager::new();
        let user_id = UserId(Uuid::now_v7());
        let mut wallet = manager.create_wallet_cap(
            user_id,
            5000, // 50.00 EUR
            "EUR".into(),
            SpendingCategory::Food,
            future_ts(),
        );

        assert_eq!(manager.remaining_budget(&wallet), 5000);
        assert!(manager.check_budget(&wallet, 3000).is_ok());

        // Spend 30.00 EUR
        manager
            .record_spend(
                &mut wallet,
                3000,
                "Lunch".into(),
                "EUR",
                SpendingCategory::Food,
            )
            .expect("spend should succeed");

        assert_eq!(manager.remaining_budget(&wallet), 2000);
        assert_eq!(manager.history().len(), 1);

        // Try to spend 25.00 EUR (exceeds remaining 20.00 EUR)
        assert!(manager.check_budget(&wallet, 2500).is_err());
        assert!(
            manager
                .record_spend(
                    &mut wallet,
                    2500,
                    "Dinner".into(),
                    "EUR",
                    SpendingCategory::Food
                )
                .is_err()
        );

        // Exact remaining spend of 20.00 EUR
        manager
            .record_spend(
                &mut wallet,
                2000,
                "Snacks".into(),
                "EUR",
                SpendingCategory::Food,
            )
            .expect("exact budget spend should succeed");
        assert_eq!(manager.remaining_budget(&wallet), 0);

        // Any further spend should fail
        assert!(
            manager
                .record_spend(&mut wallet, 1, "Gum".into(), "EUR", SpendingCategory::Food)
                .is_err()
        );
    }

    #[test]
    fn test_wallet_expiry_is_enforced() {
        let manager = WalletManager::new();
        let user_id = UserId(Uuid::now_v7());
        let expired = manager.create_wallet_cap(
            user_id,
            5000,
            "EUR".into(),
            SpendingCategory::Food,
            Timestamp(chrono::Utc::now() - chrono::Duration::seconds(10)),
        );
        assert!(matches!(
            manager.check_budget(&expired, 100),
            Err(HakilleError::CapabilityExpired)
        ));

        let mut manager = manager;
        let mut expired = expired;
        assert!(
            manager
                .record_spend(
                    &mut expired,
                    100,
                    "Late".into(),
                    "EUR",
                    SpendingCategory::Food
                )
                .is_err()
        );
    }

    #[test]
    fn test_wallet_currency_and_category_mismatch_rejected() {
        let mut manager = WalletManager::new();
        let user_id = UserId(Uuid::now_v7());
        let mut wallet = manager.create_wallet_cap(
            user_id,
            5000,
            "EUR".into(),
            SpendingCategory::Food,
            future_ts(),
        );

        // Mauvaise devise.
        assert!(
            manager
                .check_budget_for(&wallet, 100, "USD", &SpendingCategory::Food)
                .is_err()
        );
        assert!(
            manager
                .record_spend(
                    &mut wallet,
                    100,
                    "US lunch".into(),
                    "USD",
                    SpendingCategory::Food
                )
                .is_err()
        );

        // Mauvaise catégorie.
        assert!(
            manager
                .check_budget_for(&wallet, 100, "EUR", &SpendingCategory::Transport)
                .is_err()
        );
        assert!(
            manager
                .record_spend(
                    &mut wallet,
                    100,
                    "Taxi".into(),
                    "EUR",
                    SpendingCategory::Transport
                )
                .is_err()
        );

        // Scope non-Wallet ou devise vide : refus structurel.
        let mut bad_scope = wallet.clone();
        bad_scope.scope = CapabilityScope::Network;
        assert!(manager.check_budget(&bad_scope, 100).is_err());
        let mut no_currency = wallet.clone();
        no_currency.currency.clear();
        assert!(manager.check_budget(&no_currency, 100).is_err());

        // Rien n'a été dépensé après ces refus.
        assert_eq!(manager.remaining_budget(&wallet), 5000);
        assert!(manager.history().is_empty());
    }
}
