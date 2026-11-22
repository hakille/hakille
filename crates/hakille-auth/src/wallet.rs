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
        let projected_total = current_spent.checked_add(amount).ok_or_else(|| {
            HakilleError::CapabilityDenied(format!(
                "Wallet budget arithmetic overflow: requested {}, current spent {}",
                amount, current_spent
            ))
        })?;
        if projected_total > wallet.max_amount_cents {
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
        *current_spent = current_spent.saturating_add(amount);
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
            .fold(0u64, |acc, r| acc.saturating_add(r.amount))
    }

    /// Plafond journalier global : refuse si `déjà_dépensé_aujourd'hui + amount > daily_limit_cents`.
    pub fn check_daily_cap(&self, amount: u64, daily_limit_cents: u64) -> Result<()> {
        let spent_today = self.daily_spent_cents();
        let projected = spent_today.checked_add(amount).ok_or_else(|| {
            HakilleError::CapabilityDenied(format!(
                "Plafond journalier dépassé (overflow) : déjà {}c + {}c > limite {}c",
                spent_today, amount, daily_limit_cents
            ))
        })?;
        if projected > daily_limit_cents {
            return Err(HakilleError::CapabilityDenied(format!(
                "Plafond journalier dépassé : déjà {}c + {}c > limite {}c",
                spent_today, amount, daily_limit_cents
            )));
        }
        Ok(())
    }

    /// Persiste `spent` + `records` en JSON (snapshot) par écriture atomique.
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let snap = WalletSnapshot {
            spent: self.spent.iter().map(|(k, v)| (k.clone(), *v)).collect(),
            records: self.records.clone(),
        };
        let json = serde_json::to_vec_pretty(&snap)
            .map_err(|e| HakilleError::SerializationError(e.to_string()))?;

        let tmp_path = path.with_extension("json.tmp");
        {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&tmp_path)
                .map_err(|e| HakilleError::StorageError(e.to_string()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
            }
            file.write_all(&json)
                .map_err(|e| HakilleError::StorageError(e.to_string()))?;
            file.sync_all()
                .map_err(|e| HakilleError::StorageError(e.to_string()))?;
        }
        std::fs::rename(&tmp_path, path).map_err(|e| HakilleError::StorageError(e.to_string()))?;
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

    /// Strict Constitutional Rule 7 & Privilege Step-Up Guard (§30e, F21, F23).
    /// Enforces:
    /// 1. Minors under 18 and holders of Restriction::NoWallet are unconditionally denied.
    /// 2. Session confidence must be finite and satisfy tau >= 0.90 for high-privilege crypto-wallet operations.
    pub fn verify_stepup_and_rule7(
        &self,
        session_tau: f32,
        is_minor: bool,
        restrictions: &[crate::household::Restriction],
    ) -> Result<()> {
        if is_minor || restrictions.contains(&crate::household::Restriction::NoWallet) {
            return Err(HakilleError::CapabilityDenied(
                "Constitutional Rule 7: Minors under 18 are strictly prohibited from crypto-wallet operations (NoWallet invariant).".to_string(),
            ));
        }

        if !session_tau.is_finite() || session_tau < 0.90 {
            return Err(HakilleError::CapabilityDenied(format!(
                "Privilege step-up required: crypto-wallet operations require finite session confidence >= 0.90, current is {:.2}",
                session_tau
            )));
        }

        Ok(())
    }

    /// Provision a guarded wallet capability requiring step-up authentication (tau >= 0.90)
    /// and strictly enforcing the Rule 7 Minor NoWallet invariant.
    #[allow(clippy::too_many_arguments)]
    pub fn create_guarded_wallet_cap(
        &self,
        user_id: UserId,
        max_amount: u64,
        currency: String,
        category: SpendingCategory,
        expires: Timestamp,
        session_tau: f32,
        is_minor: bool,
        restrictions: &[crate::household::Restriction],
    ) -> Result<WalletCapability> {
        self.verify_stepup_and_rule7(session_tau, is_minor, restrictions)?;
        Ok(self.create_wallet_cap(user_id, max_amount, currency, category, expires))
    }

    /// Authorize and cryptographically sign a financial transaction.
    /// Invariants enforced:
    /// - Rule 7: Absolute denial for minors
    /// - F21: Continuous confidence tau >= 0.90 (and finite)
    /// - Budget limit within wallet capability
    /// - Currency and category match
    /// - Ledger spend record appended
    pub fn authorize_and_sign_transaction(
        &mut self,
        wallet: &mut WalletCapability,
        tx: WalletTransaction,
        session_tau: f32,
        is_minor: bool,
        restrictions: &[crate::household::Restriction],
        signing_key: &[u8; 32],
    ) -> Result<SignedTransactionProof> {
        self.verify_stepup_and_rule7(session_tau, is_minor, restrictions)?;

        self.record_spend(
            wallet,
            tx.amount_cents,
            tx.description.clone(),
            &tx.currency,
            tx.category.clone(),
        )?;

        let signature_digest = tx.compute_hmac_signature(signing_key);

        Ok(SignedTransactionProof {
            transaction: tx,
            signature_digest,
            authorized_at_confidence: session_tau,
            authorized_at: Timestamp::now(),
        })
    }
}

/// Transaction payload for crypto-wallet signing and transfers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletTransaction {
    pub tx_id: uuid::Uuid,
    pub recipient: String,
    pub amount_cents: u64,
    pub currency: String,
    pub category: SpendingCategory,
    pub description: String,
    pub nonce: u64,
    pub timestamp: Timestamp,
}

impl WalletTransaction {
    /// Computes a domain-separated, length-prefixed HMAC-SHA256 signature over the canonical transaction fields.
    pub fn compute_hmac_signature(&self, signing_key: &[u8; 32]) -> [u8; 32] {
        use sha2::{Digest, Sha256};

        let mut ipad = [0x36u8; 64];
        let mut opad = [0x5cu8; 64];
        for i in 0..32 {
            ipad[i] ^= signing_key[i];
            opad[i] ^= signing_key[i];
        }

        let mut inner = Sha256::new();
        inner.update(ipad);
        inner.update(b"hakille-wallet-tx-v1:");
        inner.update(self.tx_id.as_bytes());
        inner.update(self.amount_cents.to_le_bytes());
        inner.update((self.currency.len() as u64).to_le_bytes());
        inner.update(self.currency.as_bytes());
        inner.update((self.recipient.len() as u64).to_le_bytes());
        inner.update(self.recipient.as_bytes());
        inner.update((self.description.len() as u64).to_le_bytes());
        inner.update(self.description.as_bytes());
        let cat_str = format!("{:?}", self.category);
        inner.update((cat_str.len() as u64).to_le_bytes());
        inner.update(cat_str.as_bytes());
        inner.update(self.nonce.to_le_bytes());
        inner.update(self.timestamp.0.timestamp_millis().to_le_bytes());
        let inner_digest = inner.finalize();

        let mut outer = Sha256::new();
        outer.update(opad);
        outer.update(inner_digest);
        let digest = outer.finalize();

        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        out
    }
}

/// Attestation proof of an authorized, signed wallet transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedTransactionProof {
    pub transaction: WalletTransaction,
    pub signature_digest: [u8; 32],
    pub authorized_at_confidence: f32,
    pub authorized_at: Timestamp,
}

impl SignedTransactionProof {
    /// Constant-time verification of the transaction proof signature against `signing_key`.
    pub fn verify(&self, signing_key: &[u8; 32]) -> bool {
        let expected = self.transaction.compute_hmac_signature(signing_key);
        let mut diff = 0u8;
        for (a, b) in expected.iter().zip(self.signature_digest.iter()) {
            diff |= a ^ b;
        }
        diff == 0
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

    #[test]
    fn test_guarded_wallet_cap_stepup_and_rule7() {
        let manager = WalletManager::new();
        let adult_id = UserId(Uuid::new_v4());
        let minor_id = UserId(Uuid::new_v4());

        // 1. Minor user strictly rejected regardless of confidence
        let res_minor = manager.create_guarded_wallet_cap(
            minor_id,
            10000,
            "EUR".into(),
            SpendingCategory::Food,
            future_ts(),
            0.99,
            true, // is_minor
            &[crate::household::Restriction::NoWallet],
        );
        assert!(res_minor.is_err());

        // 2. Adult with insufficient confidence (< 0.90) rejected
        let res_low_tau = manager.create_guarded_wallet_cap(
            adult_id.clone(),
            10000,
            "EUR".into(),
            SpendingCategory::Food,
            future_ts(),
            0.85,
            false,
            &[],
        );
        assert!(res_low_tau.is_err());

        // 3. Adult with tau >= 0.90 and no minor restriction succeeds
        let res_ok = manager.create_guarded_wallet_cap(
            adult_id,
            10000,
            "EUR".into(),
            SpendingCategory::Food,
            future_ts(),
            0.95,
            false,
            &[],
        );
        assert!(res_ok.is_ok());
    }

    #[test]
    fn test_authorize_and_sign_transaction() {
        let mut manager = WalletManager::new();
        let user_id = UserId(Uuid::new_v4());
        let mut wallet = manager.create_wallet_cap(
            user_id,
            10000,
            "EUR".into(),
            SpendingCategory::Food,
            future_ts(),
        );

        let tx = WalletTransaction {
            tx_id: Uuid::new_v4(),
            recipient: "0x1234567890abcdef".to_string(),
            amount_cents: 2500,
            currency: "EUR".to_string(),
            category: SpendingCategory::Food,
            description: "Team Lunch".to_string(),
            nonce: 1,
            timestamp: Timestamp::now(),
        };

        let signing_key = [0xabu8; 32];

        // Minor attempt rejected
        let res_minor = manager.authorize_and_sign_transaction(
            &mut wallet,
            tx.clone(),
            0.95,
            true, // is_minor
            &[crate::household::Restriction::NoWallet],
            &signing_key,
        );
        assert!(res_minor.is_err());

        // Insufficient confidence rejected
        let res_low_tau = manager.authorize_and_sign_transaction(
            &mut wallet,
            tx.clone(),
            0.88,
            false,
            &[],
            &signing_key,
        );
        assert!(res_low_tau.is_err());

        // Valid authorization succeeds and signs
        let proof = manager
            .authorize_and_sign_transaction(&mut wallet, tx.clone(), 0.95, false, &[], &signing_key)
            .expect("Signing transaction must succeed");

        assert_eq!(proof.transaction.amount_cents, 2500);
        assert_ne!(proof.signature_digest, [0u8; 32]);
        assert_eq!(manager.remaining_budget(&wallet), 7500);
        assert_eq!(manager.history().len(), 1);
    }
}
