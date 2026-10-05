pub mod bounded_initiative;
pub mod continuity;
pub mod drift;
pub mod generality;

pub use bounded_initiative::*;
pub use continuity::*;
pub use drift::*;
pub use generality::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_continuity_tracker_rate() {
        let mut tracker = ContinuityTracker::new();
        assert_eq!(tracker.continuity_rate(), 1.0);

        // 3 tasks without re-explanation, 1 requiring re-explanation
        tracker.record_task("t1", false, 1);
        tracker.record_task("t2", false, 2);
        tracker.record_task("t3", false, 1);
        tracker.record_task("t4", true, 4);

        assert_eq!(tracker.total_tasks(), 4);
        assert_eq!(tracker.continuity_rate(), 0.75);
    }

    #[test]
    fn test_bounded_initiative_compliance() {
        let mut tracker = BoundedInitiativeTracker::new();
        assert_eq!(tracker.compliance_rate(), 1.0);

        // 9 compliant proposals, 1 exceeding capability
        for i in 0..9 {
            tracker.record_proposal(format!("p{}", i), true, true);
        }
        tracker.record_proposal("p_bad", false, false);

        assert_eq!(tracker.compliance_rate(), 0.9);
    }

    #[test]
    fn test_generality_tracker() {
        let mut tracker = GeneralityTracker::new();
        // Initially empty
        assert_eq!(tracker.generality_score(), 0.0);

        tracker.record_task(TaskDomain::Planning);
        tracker.record_task(TaskDomain::Coding);
        tracker.record_task(TaskDomain::Writing);
        tracker.record_task(TaskDomain::Automation);

        // Diversity score should be positive
        assert!(tracker.generality_score() > 0.5);
    }

    #[test]
    fn test_values_drift_detection() {
        let detector = ValuesDriftDetector::default_constitutional();

        // Identical vector: no drift
        assert!(detector.evaluate_current_vector(&[1.0, 1.0, 1.0, 1.0, 1.0]).is_none());

        // Mild vector: under 0.15 threshold
        assert!(detector.evaluate_current_vector(&[0.95, 1.0, 0.98, 1.0, 0.99]).is_none());

        // Strongly drifted vector: should trigger alert
        let drifted = detector.evaluate_current_vector(&[1.0, 0.0, 0.0, 0.0, 0.0]);
        assert!(drifted.is_some());
        let alert = drifted.unwrap();
        assert!(alert.cosine_drift > alert.threshold);
    }
}
