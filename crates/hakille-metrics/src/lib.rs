pub mod bounded_initiative;
pub mod continuity;
pub mod drift;
pub mod generality;

pub use bounded_initiative::*;
pub use continuity::*;
pub use drift::*;
pub use generality::*;

use serde::{Deserialize, Serialize};

/// Snapshot global sérialisable de tous les trackers (persistance mémoire simple).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub generality: GeneralitySnapshot,
    pub continuity: ContinuitySnapshot,
    pub bounded_initiative: BoundedInitiativeSnapshot,
    pub values_drift: ValuesDriftSnapshot,
}

impl MetricsSnapshot {
    /// Construit un snapshot à partir des trackers vivants.
    pub fn collect(
        generality: &GeneralityTracker,
        continuity: &ContinuityTracker,
        bounded_initiative: &BoundedInitiativeTracker,
        values_drift: &ValuesDriftDetector,
    ) -> Self {
        Self {
            generality: generality.to_snapshot(),
            continuity: continuity.to_snapshot(),
            bounded_initiative: bounded_initiative.to_snapshot(),
            values_drift: values_drift.to_snapshot(),
        }
    }

    /// Alias `to_snapshot` pour compatibilité avec la convention des trackers.
    pub fn to_snapshot(&self) -> Self {
        self.clone()
    }

    /// Persistance JSON atomique via fichier temporaire.
    pub fn save_to_file(&self, path: &std::path::Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        atomic_write_json(path, &json)
    }

    pub fn load_from_file(path: &std::path::Path) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let snap: Self = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(snap)
    }
}

pub(crate) fn atomic_write_json(path: &std::path::Path, json: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));
    {
        use std::io::Write;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file = opts.open(&tmp_path)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp_path, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_continuity_tracker_rate() {
        let mut tracker = ContinuityTracker::new();
        assert!(!tracker.has_measurements());
        assert_eq!(tracker.continuity_rate_checked(), None);
        assert_eq!(tracker.continuity_rate(), 1.0);

        // 3 tasks without re-explanation, 1 requiring re-explanation
        tracker.record_task("t1", false, 1);
        tracker.record_task("t2", false, 2);
        tracker.record_task("t3", false, 1);
        tracker.record_task("t4", true, 4);

        assert!(tracker.has_measurements());
        assert_eq!(tracker.total_tasks(), 4);
        assert_eq!(tracker.continuity_rate(), 0.75);
        assert_eq!(tracker.continuity_rate_checked(), Some(0.75));
    }

    #[test]
    fn test_bounded_initiative_compliance() {
        let mut tracker = BoundedInitiativeTracker::new();
        assert!(!tracker.has_measurements());
        assert_eq!(tracker.compliance_rate_checked(), None);
        assert_eq!(tracker.acceptance_rate_checked(), None);
        assert_eq!(tracker.compliance_rate(), 1.0);

        // 9 compliant proposals, 1 exceeding capability
        for i in 0..9 {
            tracker.record_proposal(format!("p{}", i), true, true);
        }
        tracker.record_proposal("p_bad", false, false);

        assert!(tracker.has_measurements());
        assert_eq!(tracker.compliance_rate(), 0.9);
        assert_eq!(tracker.compliance_rate_checked(), Some(0.9));
        assert_eq!(tracker.acceptance_rate_checked(), Some(0.9));
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
        assert!(
            detector
                .evaluate_current_vector(&[1.0, 1.0, 1.0, 1.0, 1.0])
                .is_none()
        );

        // Mild vector: under 0.15 threshold
        assert!(
            detector
                .evaluate_current_vector(&[0.95, 1.0, 0.98, 1.0, 0.99])
                .is_none()
        );

        // Strongly drifted vector: should trigger alert
        let drifted = detector.evaluate_current_vector(&[1.0, 0.0, 0.0, 0.0, 0.0]);
        assert!(drifted.is_some());
        let alert = drifted.unwrap();
        assert!(alert.cosine_drift > alert.threshold);

        // NaN or zero vector must fail closed with an alert
        assert!(
            detector
                .evaluate_current_vector(&[f64::NAN, 1.0, 1.0, 1.0, 1.0])
                .is_some()
        );
        assert!(
            detector
                .evaluate_current_vector(&[0.0, 0.0, 0.0, 0.0, 0.0])
                .is_some()
        );
    }
}
