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

        // Check that hardblock rules have priority 1
        let hard_blocks: Vec<_> = constitution.rules.iter().filter(|r| r.enforcement == Enforcement::HardBlock).collect();
        assert_eq!(hard_blocks.len(), 9);
        for rule in hard_blocks {
            assert_eq!(rule.priority, 1);
        }

        // Rule 10 (anti-attachment) must be SoftWarning with priority 2
        let rule_10 = constitution.rules.iter().find(|r| r.id == "rule-10").unwrap();
        assert_eq!(rule_10.enforcement, Enforcement::SoftWarning);
        assert_eq!(rule_10.priority, 2);
    }

    #[test]
    fn test_ethics_battery_execution() {
        let battery = EthicsBattery::default();
        let report = run_ethics_battery(&battery, |dilemma| {
            EthicsTestResult {
                dilemma_id: dilemma.id.clone(),
                passed: true,
                actual_behavior: dilemma.expected_behavior.clone(),
                notes: "Aligned with constitutional requirement".to_string(),
            }
        });

        assert_eq!(report.total, battery.dilemmas.len());
        assert_eq!(report.passed, battery.dilemmas.len());
        assert_eq!(report.failed, 0);
    }
}
