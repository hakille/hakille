use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatedDiff {
    pub target_resource: String,
    pub state_before: String,
    pub state_after: String,
    pub estimated_impact: String,
    pub is_destructive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DryRunReport {
    pub action_type: String,
    pub simulation_successful: bool,
    pub diffs: Vec<SimulatedDiff>,
    pub warning: Option<String>,
    pub human_readable_preview: String,
}

/// Dry-Run Simulation Sandbox (§24d).
/// Replays actions locally before committing irreversible external mutations.
pub struct DryRunSimulator;

impl DryRunSimulator {
    /// Simulate a financial payment transaction before committing.
    pub fn simulate_payment(
        recipient: &str,
        amount_cents: u64,
        currency: &str,
        current_balance_cents: u64,
    ) -> DryRunReport {
        let after_balance = current_balance_cents.saturating_sub(amount_cents);
        let amount_formatted = format!("{:.2} {}", amount_cents as f64 / 100.0, currency);
        let before_formatted = format!("{:.2} {}", current_balance_cents as f64 / 100.0, currency);
        let after_formatted = format!("{:.2} {}", after_balance as f64 / 100.0, currency);

        let diff = SimulatedDiff {
            target_resource: format!("Wallet / {}", recipient),
            state_before: before_formatted,
            state_after: after_formatted.clone(),
            estimated_impact: format!("Transfert de {} vers {}", amount_formatted, recipient),
            is_destructive: false,
        };

        DryRunReport {
            action_type: "WalletPayment".to_string(),
            simulation_successful: true,
            diffs: vec![diff],
            warning: if amount_cents > 10000 {
                Some(
                    "Montant supérieur à 100.00 EUR : validation humaine explicite requise (§19e)."
                        .to_string(),
                )
            } else {
                None
            },
            human_readable_preview: format!(
                "Simulation : virement de {} vers '{}'. Solde restant estimé : {}.",
                amount_formatted, recipient, after_formatted
            ),
        }
    }

    /// Simulate a file modification or deletion before committing.
    pub fn simulate_file_mutation(
        path: &str,
        is_deletion: bool,
        old_size_bytes: usize,
        new_size_bytes: usize,
    ) -> DryRunReport {
        let diff = SimulatedDiff {
            target_resource: path.to_string(),
            state_before: format!("{} octets", old_size_bytes),
            state_after: if is_deletion {
                "Supprimé (0 octet)".to_string()
            } else {
                format!("{} octets", new_size_bytes)
            },
            estimated_impact: if is_deletion {
                "Suppression définitive du fichier".to_string()
            } else {
                "Écriture / remplacement du contenu".to_string()
            },
            is_destructive: is_deletion,
        };

        DryRunReport {
            action_type: if is_deletion {
                "FileDelete".to_string()
            } else {
                "FileWrite".to_string()
            },
            simulation_successful: true,
            diffs: vec![diff],
            warning: if is_deletion {
                Some("Suppression irréversible : validation explicite obligatoire.".to_string())
            } else {
                None
            },
            human_readable_preview: format!(
                "Simulation : {} sur '{}'.",
                if is_deletion {
                    "suppression"
                } else {
                    "modification"
                },
                path
            ),
        }
    }
}
