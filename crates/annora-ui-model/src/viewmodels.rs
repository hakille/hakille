//! View-models immuables — canoniques, sérialisables, sans Slint.
//!
//! Parité stricte avec `annora-ui-shell::views` (tests de parité ci-dessous).
//! Le renderer ne reçoit que ces structs.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalRow {
    pub id: String,
    pub time: String,
    pub actor: String,
    pub action: String,
    pub reason: String,
    pub confidence_pct: u8,
    pub cost: String,
    pub verified: bool,
    pub can_revert: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRow {
    pub id: String,
    pub text: String,
    pub partition: String,
    pub confidence_pct: u8,
}

impl MemoryRow {
    pub fn freshness_label(&self) -> &'static str {
        let c = self.confidence_pct as f32 / 100.0;
        if c >= 0.8 {
            "frais"
        } else if c >= 0.4 {
            "récent"
        } else {
            "fané"
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerRow {
    pub group: String,
    pub scope: String,
    pub enabled: bool,
    pub policy_locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowersView {
    pub rows: Vec<PowerRow>,
}

impl PowersView {
    pub fn kill_all(&mut self) {
        for r in &mut self.rows {
            if !r.policy_locked {
                r.enabled = false;
            }
        }
    }
}

pub fn filter_journal<'a>(rows: &'a [JournalRow], query: &str) -> Vec<&'a JournalRow> {
    let q = query.to_lowercase();
    rows.iter()
        .filter(|r| r.action.to_lowercase().contains(&q) || r.reason.to_lowercase().contains(&q))
        .collect()
}

pub fn page_journal(rows: &[JournalRow], page: usize, per_page: usize) -> &[JournalRow] {
    let start = page.saturating_mul(per_page).min(rows.len());
    let end = (start + per_page).min(rows.len());
    &rows[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_filter_and_page() {
        let rows = vec![
            JournalRow {
                id: "1".into(),
                time: "10:00".into(),
                actor: "a".into(),
                action: "Envoyer mail".into(),
                reason: "demandé".into(),
                confidence_pct: 90,
                cost: "0.00".into(),
                verified: true,
                can_revert: true,
            },
            JournalRow {
                id: "2".into(),
                time: "10:01".into(),
                actor: "a".into(),
                action: "Payer facture".into(),
                reason: "auto".into(),
                confidence_pct: 40,
                cost: "1.00".into(),
                verified: false,
                can_revert: false,
            },
        ];
        assert_eq!(filter_journal(&rows, "mail").len(), 1);
        assert_eq!(page_journal(&rows, 0, 1).len(), 1);
        assert_eq!(page_journal(&rows, 5, 10).len(), 0);
    }

    #[test]
    fn memory_freshness_thresholds() {
        let f = |c| {
            MemoryRow {
                id: "x".into(),
                text: "t".into(),
                partition: "p".into(),
                confidence_pct: c,
            }
            .freshness_label()
        };
        assert_eq!(f(90), "frais");
        assert_eq!(f(50), "récent");
        assert_eq!(f(10), "fané");
    }

    #[test]
    fn kill_all_respects_lock() {
        let mut v = PowersView {
            rows: vec![
                PowerRow {
                    group: "Voir".into(),
                    scope: "Camera".into(),
                    enabled: true,
                    policy_locked: false,
                },
                PowerRow {
                    group: "Agir".into(),
                    scope: "Gui".into(),
                    enabled: true,
                    policy_locked: true,
                },
            ],
        };
        v.kill_all();
        assert!(!v.rows[0].enabled);
        assert!(v.rows[1].enabled);
    }

    #[test]
    fn journal_100k_filter_budget() {
        let rows: Vec<JournalRow> = (0..100_000)
            .map(|i| JournalRow {
                id: i.to_string(),
                time: "t".into(),
                actor: "a".into(),
                action: format!("action {i}"),
                reason: "r".into(),
                confidence_pct: 80,
                cost: "0".into(),
                verified: true,
                can_revert: true,
            })
            .collect();
        let t = std::time::Instant::now();
        let out = filter_journal(&rows, "99999");
        let _ = page_journal(&rows, 0, 50);
        assert!(!out.is_empty());
        assert!(t.elapsed().as_secs() < 2, "budget 2s debug dépassé");
    }
}
