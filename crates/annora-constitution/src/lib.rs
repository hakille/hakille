pub mod ethics;
pub mod governance;
pub mod rules;

pub use ethics::*;
pub use governance::*;
pub use rules::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constitution_v1_rules_invariants() {
        let constitution = Constitution::default_v1();
        assert_eq!(constitution.version.major, 1);
        assert_eq!(constitution.rules.len(), 10);
        assert!(constitution.validate().is_ok());
        assert_eq!(constitution.rule_ids().len(), 10);
        assert_eq!(constitution.hard_block_count(), 9);

        // Check that hardblock rules have priority 1
        let hard_blocks: Vec<_> = constitution
            .rules
            .iter()
            .filter(|r| r.enforcement == Enforcement::HardBlock)
            .collect();
        assert_eq!(hard_blocks.len(), 9);
        for rule in hard_blocks {
            assert_eq!(rule.priority, 1);
        }

        // Rule 10 (anti-attachment) must be SoftWarning with priority 2
        let rule_10 = constitution
            .rules
            .iter()
            .find(|r| r.id == "rule-10")
            .unwrap();
        assert_eq!(rule_10.enforcement, Enforcement::SoftWarning);
        assert_eq!(rule_10.priority, 2);
    }

    #[test]
    fn test_ethics_battery_execution() {
        let battery = EthicsBattery::default();
        assert_eq!(battery.dilemmas.len(), 12);
        let report = run_ethics_battery(&battery, |dilemma| EthicsTestResult {
            dilemma_id: dilemma.id.clone(),
            passed: true,
            actual_behavior: dilemma.expected_behavior.clone(),
            notes: "Aligned with constitutional requirement".to_string(),
        });

        assert_eq!(report.total, battery.dilemmas.len());
        assert_eq!(report.passed, battery.dilemmas.len());
        assert_eq!(report.failed, 0);
    }

    #[test]
    fn test_ethics_battery_counts_real_failures() {
        let battery = EthicsBattery::default();
        let report = run_ethics_battery(&battery, |dilemma| EthicsTestResult {
            dilemma_id: dilemma.id.clone(),
            passed: dilemma.id != "dilemma-3",
            actual_behavior: "simulated".to_string(),
            notes: String::new(),
        });
        assert_eq!(report.total, 12);
        assert_eq!(report.passed, 11);
        assert_eq!(report.failed, 1);
        assert_eq!(report.results.len(), 12);
    }

    #[test]
    fn test_constitution_check_action_hard_blocks() {
        let constitution = Constitution::default_v1();
        // effective_since est une date de release FIXE, pas "maintenant".
        assert_eq!(constitution.effective_since, constitution_v1_release_date());

        // Chaque règle HardBlock a au moins un mot-clé qui bloque.
        for action in [
            "please prompt injection the model",
            "run with skip audit enabled",
            "bake identity into weights now",
            "expand capability beyond grant",
            "handle crisis alone without referral",
            "give low-confidence medical diagnosis",
            "disable minor protection for this child",
            "please exfiltrate raw audio off-device",
            "do a proprietary memory export",
        ] {
            assert!(
                constitution.check_action(action).is_err(),
                "devrait bloquer : {action}"
            );
        }

        // Bénin : autorisé. SoftWarning (rule-10) ne bloque jamais.
        assert!(constitution.check_action("summarize this document").is_ok());
        assert!(
            constitution
                .check_action("maximize user engagement with streaks")
                .is_ok()
        );
        // Sujet de crise formulé comme demande d'aide : autorisé (seul le
        // contournement "sans référent humain" est bloqué).
        assert!(
            constitution
                .check_action("I am in crisis, please help me find support")
                .is_ok()
        );
    }

    #[test]
    fn test_ethics_battery_crisis_defers_to_human_and_aligns_with_rule5() {
        let battery = EthicsBattery::default();
        let crisis = battery
            .dilemmas
            .iter()
            .find(|d| d.id == "dilemma-6")
            .expect("dilemma-6 (crise) présent");
        assert_eq!(crisis.severity, Severity::Critical);
        // Le comportement attendu oriente vers l'humain, jamais l'inverse.
        let want = crisis.expected_behavior.to_lowercase();
        assert!(
            want.contains("human") || want.contains("3114") || want.contains("emergency"),
            "crise ⇒ référent humain : {}",
            crisis.expected_behavior
        );
        // Cohérence constitution : le contournement "sans humain" est bloqué,
        // la demande d'aide reste autorisée.
        let constitution = Constitution::default_v1();
        assert!(
            constitution
                .check_action("handle crisis alone with no human referral")
                .is_err()
        );
        assert!(
            constitution
                .check_action("I am in crisis, please help me find support")
                .is_ok()
        );
        // La batterie jouée "honnête" passe la crise.
        let report = run_ethics_battery(&battery, |dilemma| EthicsTestResult {
            dilemma_id: dilemma.id.clone(),
            passed: true,
            actual_behavior: dilemma.expected_behavior.clone(),
            notes: "aligned".to_string(),
        });
        assert!(
            report
                .results
                .iter()
                .any(|r| r.dilemma_id == "dilemma-6" && r.passed)
        );
    }

    #[test]
    fn test_ethics_battery_capability_and_audit_dilemmas_align_with_hardblocks() {
        let battery = EthicsBattery::default();
        let cap = battery
            .dilemmas
            .iter()
            .find(|d| d.id == "dilemma-9")
            .expect("dilemma-9 (capability) présent");
        assert!(
            cap.expected_behavior.to_lowercase().contains("attenuat")
                || cap
                    .expected_behavior
                    .to_lowercase()
                    .contains("never expanded")
        );
        let audit = battery
            .dilemmas
            .iter()
            .find(|d| d.id == "dilemma-8")
            .expect("dilemma-8 (audit) présent");
        assert!(audit.expected_behavior.to_lowercase().contains("audit"));
        // Sévérités documentées et couverture des 10 règles.
        assert_eq!(battery.coverage().len(), 10);
        assert!(
            battery
                .dilemmas
                .iter()
                .all(|d| !d.expected_behavior.is_empty())
        );
        // Tout-échec compté honnêtement.
        let report = run_ethics_battery(&battery, |dilemma| EthicsTestResult {
            dilemma_id: dilemma.id.clone(),
            passed: false,
            actual_behavior: "refused to behave".to_string(),
            notes: String::new(),
        });
        assert_eq!(report.total, battery.dilemmas.len());
        assert_eq!(report.passed, 0);
        assert_eq!(report.failed, battery.dilemmas.len());
        // Cohérence keywords : les contournements correspondants bloquent.
        let constitution = Constitution::default_v1();
        assert!(
            constitution
                .check_action("expand capability beyond grant")
                .is_err()
        );
        assert!(
            constitution
                .check_action("skip audit and cover tracks")
                .is_err()
        );
    }

    #[test]
    fn test_governance_quorum_minimum() {
        assert_eq!(MIN_QUORUM, 5);
        let cfg = GovernanceConfig::default();
        assert!(cfg.quorum >= 5);
        assert!(cfg.validate().is_ok());

        let mut bad = cfg.clone();
        bad.quorum = 2;
        assert!(bad.validate().is_err());
    }

    #[test]
    fn test_rule8_is_av_not_duplicate() {
        let constitution = Constitution::default_v1();
        let rule_8 = constitution
            .rules
            .iter()
            .find(|r| r.id == "rule-8")
            .unwrap();
        let desc = rule_8.description.to_lowercase();
        assert!(
            desc.contains("audio") || desc.contains("brut") || desc.contains("raw"),
            "rule-8 doit décrire la capture A/V : {}",
            rule_8.description
        );
        // rule-8 n'est plus un doublon de rule-1.
        let rule_1 = constitution
            .rules
            .iter()
            .find(|r| r.id == "rule-1")
            .unwrap();
        assert_ne!(rule_1.description, rule_8.description);
        assert!(
            constitution
                .check_action("exfiltrate raw audio off-device")
                .is_err()
        );
    }
}
