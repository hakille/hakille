use annora_core::{CapabilityAction, CapabilityScope};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionType {
    Debit,
    Credit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BudgetCategory {
    Housing,
    Groceries,
    Utilities,
    Transport,
    Entertainment,
    Health,
    Income,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinancialTransaction {
    pub id: String,
    pub timestamp_rfc3339: String,
    pub amount_cents: u64,
    pub tx_type: TransactionType,
    pub merchant: String,
    pub category: BudgetCategory,
    pub memo: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountStatement {
    pub account_id: String,
    pub account_holder: String,
    pub iban_masked: String,
    pub currency: String,
    pub balance_cents: u64,
    pub transactions: Vec<FinancialTransaction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetSummary {
    pub total_spent_cents: u64,
    pub total_credited_cents: u64,
    pub spending_by_category: HashMap<BudgetCategory, u64>,
}

pub struct WalletReadOnlyClient {
    accounts: Vec<AccountStatement>,
}

impl WalletReadOnlyClient {
    pub fn new() -> Self {
        let checking = AccountStatement {
            account_id: "acc-fr-001".to_string(),
            account_holder: "Amadou - Compte Courant".to_string(),
            iban_masked: "FR76 •••• •••• •••• 4821".to_string(),
            currency: "EUR".to_string(),
            balance_cents: 345_050, // 3 450.50 €
            transactions: vec![
                FinancialTransaction {
                    id: "tx-001".to_string(),
                    timestamp_rfc3339: "2026-10-01T09:30:00Z".to_string(),
                    amount_cents: 280_000,
                    tx_type: TransactionType::Credit,
                    merchant: "Salaire Entreprise".to_string(),
                    category: BudgetCategory::Income,
                    memo: "Virement Salaire Octobre".to_string(),
                },
                FinancialTransaction {
                    id: "tx-002".to_string(),
                    timestamp_rfc3339: "2026-10-02T12:45:00Z".to_string(),
                    amount_cents: 8_500,
                    tx_type: TransactionType::Debit,
                    merchant: "Biocoop Paris".to_string(),
                    category: BudgetCategory::Groceries,
                    memo: "Courses biologiques".to_string(),
                },
                FinancialTransaction {
                    id: "tx-003".to_string(),
                    timestamp_rfc3339: "2026-10-04T18:15:00Z".to_string(),
                    amount_cents: 4_200,
                    tx_type: TransactionType::Debit,
                    merchant: "SNCF Connect".to_string(),
                    category: BudgetCategory::Transport,
                    memo: "Billet TGV InOui".to_string(),
                },
                FinancialTransaction {
                    id: "tx-004".to_string(),
                    timestamp_rfc3339: "2026-10-05T20:00:00Z".to_string(),
                    amount_cents: 3_500,
                    tx_type: TransactionType::Debit,
                    merchant: "Cinéma UGC".to_string(),
                    category: BudgetCategory::Entertainment,
                    memo: "Deux places séance du soir".to_string(),
                },
            ],
        };

        let savings = AccountStatement {
            account_id: "acc-fr-002".to_string(),
            account_holder: "Amadou - Livret A Souverain".to_string(),
            iban_masked: "FR76 •••• •••• •••• 9912".to_string(),
            currency: "EUR".to_string(),
            balance_cents: 1_250_000, // 12 500.00 €
            transactions: Vec::new(),
        };

        Self {
            accounts: vec![checking, savings],
        }
    }

    pub fn get_accounts(&self) -> &[AccountStatement] {
        &self.accounts
    }

    pub fn get_statement(&self, account_id: &str) -> Option<&AccountStatement> {
        self.accounts.iter().find(|a| a.account_id == account_id)
    }

    pub fn calculate_budget_summary(&self, account_id: &str) -> Option<BudgetSummary> {
        let account = self.get_statement(account_id)?;
        let mut total_spent = 0;
        let mut total_credited = 0;
        let mut by_category = HashMap::new();

        for tx in &account.transactions {
            match tx.tx_type {
                TransactionType::Debit => {
                    total_spent += tx.amount_cents;
                    *by_category.entry(tx.category).or_insert(0) += tx.amount_cents;
                }
                TransactionType::Credit => {
                    total_credited += tx.amount_cents;
                }
            }
        }

        Some(BudgetSummary {
            total_spent_cents: total_spent,
            total_credited_cents: total_credited,
            spending_by_category: by_category,
        })
    }

    pub fn search_transactions(
        &self,
        query: &str,
    ) -> Vec<(&AccountStatement, &FinancialTransaction)> {
        let q_lower = query.to_lowercase();
        let mut matches = Vec::new();

        for acc in &self.accounts {
            for tx in &acc.transactions {
                if tx.merchant.to_lowercase().contains(&q_lower)
                    || tx.memo.to_lowercase().contains(&q_lower)
                {
                    matches.push((acc, tx));
                }
            }
        }

        matches
    }

    // =========================================================================
    // CONSTITUTIONAL HARD INVARIANTS: ABSOLUTE IMMUTABILITY & TRANSFER FORBIDDEN
    // =========================================================================

    pub fn attempt_fund_transfer(
        &self,
        _target_iban: &str,
        _amount_cents: u64,
    ) -> Result<(), &'static str> {
        Err(
            "HARD SECURITY INVARIANT: Wallet agent is strictly Read-Only. Direct transfer or fund mutation operations are permanently forbidden at the architectural level.",
        )
    }

    pub fn attempt_write_transaction(
        &mut self,
        _tx: &FinancialTransaction,
    ) -> Result<(), &'static str> {
        Err(
            "HARD SECURITY INVARIANT: Wallet agent is strictly Read-Only. Transaction injection is prohibited.",
        )
    }
}

impl Default for WalletReadOnlyClient {
    fn default() -> Self {
        Self::new()
    }
}

pub fn check_wallet_capabilities() -> Vec<(CapabilityScope, CapabilityAction)> {
    // Only Read action is permitted for Wallet scope!
    vec![(CapabilityScope::Wallet, CapabilityAction::Read)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallet_read_and_budget_analysis() {
        let client = WalletReadOnlyClient::new();
        let accounts = client.get_accounts();
        assert_eq!(accounts.len(), 2);

        let checking = client.get_statement("acc-fr-001").unwrap();
        assert_eq!(checking.balance_cents, 345_050);
        assert_eq!(checking.transactions.len(), 4);

        let budget = client.calculate_budget_summary("acc-fr-001").unwrap();
        assert_eq!(budget.total_credited_cents, 280_000);
        assert_eq!(budget.total_spent_cents, 8_500 + 4_200 + 3_500);
        assert_eq!(
            budget.spending_by_category.get(&BudgetCategory::Groceries),
            Some(&8_500)
        );
        assert_eq!(
            budget.spending_by_category.get(&BudgetCategory::Transport),
            Some(&4_200)
        );

        let search = client.search_transactions("biocoop");
        assert_eq!(search.len(), 1);
        assert_eq!(search[0].1.merchant, "Biocoop Paris");
    }

    #[test]
    fn test_constitutional_refusal_of_transfers_and_writes() {
        let mut client = WalletReadOnlyClient::new();

        // 1. Fund transfer must fail with the explicit invariant error
        let transfer_res = client.attempt_fund_transfer("FR76 1234 5678 9012 3456", 5000);
        assert!(transfer_res.is_err());
        assert_eq!(
            transfer_res.unwrap_err(),
            "HARD SECURITY INVARIANT: Wallet agent is strictly Read-Only. Direct transfer or fund mutation operations are permanently forbidden at the architectural level."
        );

        // 2. Transaction injection must fail with the explicit invariant error
        let fake_tx = FinancialTransaction {
            id: "tx-fake".to_string(),
            timestamp_rfc3339: "2026-10-08T00:00:00Z".to_string(),
            amount_cents: 100_000,
            tx_type: TransactionType::Credit,
            merchant: "Exploit".to_string(),
            category: BudgetCategory::Income,
            memo: "Fake deposit".to_string(),
        };
        let write_res = client.attempt_write_transaction(&fake_tx);
        assert!(write_res.is_err());
        assert_eq!(
            write_res.unwrap_err(),
            "HARD SECURITY INVARIANT: Wallet agent is strictly Read-Only. Transaction injection is prohibited."
        );

        // 3. Check capabilities contains Read and NEVER Write
        let caps = check_wallet_capabilities();
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0], (CapabilityScope::Wallet, CapabilityAction::Read));
        assert!(!caps.contains(&(CapabilityScope::Wallet, CapabilityAction::Write)));
    }
}
