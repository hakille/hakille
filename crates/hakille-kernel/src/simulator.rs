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
        // Solde insuffisant : la simulation DOIT échouer honnêtement (§24d :
        // le dry-run prédit le résultat réel, il ne flatte pas l'action).
        let sufficient_funds = amount_cents <= current_balance_cents;
        let after_balance = current_balance_cents.saturating_sub(amount_cents);
        let amount_formatted = format!("{:.2} {}", amount_cents as f64 / 100.0, currency);
        let before_formatted = format!("{:.2} {}", current_balance_cents as f64 / 100.0, currency);
        let after_formatted = format!("{:.2} {}", after_balance as f64 / 100.0, currency);

        let diff = SimulatedDiff {
            target_resource: format!("Wallet / {}", recipient),
            state_before: before_formatted.clone(),
            state_after: if sufficient_funds {
                after_formatted.clone()
            } else {
                format!("Rejeté — solde insuffisant ({before_formatted})")
            },
            estimated_impact: if sufficient_funds {
                format!("Transfert de {} vers {}", amount_formatted, recipient)
            } else {
                format!(
                    "Transfert de {} vers {} IMPOSSIBLE (solde insuffisant)",
                    amount_formatted, recipient
                )
            },
            is_destructive: false,
        };

        let warning = if !sufficient_funds {
            Some(format!(
                "Solde insuffisant : {} requis, {} disponibles. Le virement serait rejeté.",
                amount_formatted, before_formatted
            ))
        } else if amount_cents > 10000 {
            Some(
                "Montant supérieur à 100.00 EUR : validation humaine explicite requise (§19e)."
                    .to_string(),
            )
        } else {
            None
        };

        DryRunReport {
            action_type: "WalletPayment".to_string(),
            simulation_successful: sufficient_funds,
            diffs: vec![diff],
            warning,
            human_readable_preview: if sufficient_funds {
                format!(
                    "Simulation : virement de {} vers '{}'. Solde restant estimé : {}.",
                    amount_formatted, recipient, after_formatted
                )
            } else {
                format!(
                    "Simulation : virement de {} vers '{}' IMPOSSIBLE — solde insuffisant ({} disponibles).",
                    amount_formatted, recipient, before_formatted
                )
            },
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

#[cfg(test)]
mod tests {
    use super::DryRunSimulator;

    #[test]
    fn payment_within_balance_succeeds() {
        let report = DryRunSimulator::simulate_payment("Alice", 2_500, "EUR", 10_000);
        assert!(report.simulation_successful);
        assert!(report.warning.is_none());
        assert_eq!(report.diffs[0].state_after, "75.00 EUR");
    }

    #[test]
    fn payment_over_balance_fails_honestly() {
        let report = DryRunSimulator::simulate_payment("Bob", 15_000, "EUR", 10_000);
        assert!(
            !report.simulation_successful,
            "un virement supérieur au solde ne peut pas être simulé comme un succès"
        );
        let warning = report.warning.expect("warning solde insuffisant requis");
        assert!(warning.contains("Solde insuffisant"));
        assert!(report.diffs[0].state_after.contains("Rejeté"));
        assert!(report.human_readable_preview.contains("IMPOSSIBLE"));
    }

    #[test]
    fn large_payment_within_balance_warns_but_succeeds() {
        let report = DryRunSimulator::simulate_payment("Carol", 12_000, "EUR", 20_000);
        assert!(report.simulation_successful);
        assert!(
            report
                .warning
                .expect("validation humaine requise > 100 EUR")
                .contains("validation humaine")
        );
    }

    #[test]
    fn exact_balance_payment_succeeds_with_zero_left() {
        let report = DryRunSimulator::simulate_payment("Dave", 10_000, "EUR", 10_000);
        assert!(report.simulation_successful);
        assert_eq!(report.diffs[0].state_after, "0.00 EUR");
    }

    #[test]
    fn deletion_is_marked_destructive() {
        let report = DryRunSimulator::simulate_file_mutation("/etc/config.json", true, 2048, 0);
        assert!(report.simulation_successful);
        assert!(report.diffs[0].is_destructive);
        assert!(report.warning.is_some());
    }
}
