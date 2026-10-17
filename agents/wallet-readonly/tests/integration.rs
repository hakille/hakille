use agent_wallet_readonly::{
    BudgetCategory, FinancialTransaction, TransactionType, WalletReadOnlyClient,
    check_wallet_capabilities,
};
use annora_core::{CapabilityAction, CapabilityScope};

/// Intégration wallet : lecture + budget, refus write absolu (fail-closed).
#[test]
fn test_wallet_integration_readonly_refuses_write() {
    let mut client = WalletReadOnlyClient::new();
    assert_eq!(client.get_accounts().len(), 2);

    let stmt = client.get_statement("acc-fr-001").unwrap();
    assert_eq!(stmt.transactions.len(), 4);
    let budget = client.calculate_budget_summary("acc-fr-001").unwrap();
    assert_eq!(budget.total_credited_cents, 280_000);
    assert_eq!(
        budget.spending_by_category.get(&BudgetCategory::Groceries),
        Some(&8_500)
    );

    // Tentative d'écriture : refus avec invariant constitutionnel, état inchangé.
    assert!(client.attempt_fund_transfer("FR76XXXX", 1_000).is_err());
    let fake = FinancialTransaction {
        id: "tx-evil".to_string(),
        timestamp_rfc3339: "2026-10-09T00:00:00Z".to_string(),
        amount_cents: 999,
        tx_type: TransactionType::Credit,
        merchant: "Evil".to_string(),
        category: BudgetCategory::Income,
        memo: "inject".to_string(),
    };
    assert!(client.attempt_write_transaction(&fake).is_err());
    assert_eq!(
        client
            .get_statement("acc-fr-001")
            .unwrap()
            .transactions
            .len(),
        4
    );

    // Capabilities : Read seul, jamais Write.
    let caps = check_wallet_capabilities();
    assert_eq!(
        caps,
        vec![(CapabilityScope::Wallet, CapabilityAction::Read)]
    );
}
