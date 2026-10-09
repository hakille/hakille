use agent_wallet_readonly::WalletReadOnlyClient;

fn main() {
    println!("Annora Wallet (Read-Only) Agent initialized. Transaction execution is permanently forbidden.");
    let client = WalletReadOnlyClient::new();
    let accounts = client.get_accounts();
    for acc in accounts {
        println!(
            "  -> Compte: {} ({}) | Solde: {:.2} {}",
            acc.account_holder,
            acc.iban_masked,
            acc.balance_cents as f64 / 100.0,
            acc.currency
        );
    }
}
