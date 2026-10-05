use annora_audit::entry::AuditEntry;
use annora_core::EntryId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalViewItem {
    pub entry_id: EntryId,
    pub timestamp_str: String,
    pub actor_name: String,
    pub action_summary: String,
    pub reasoning_summary: String,
    pub confidence_percentage: u8,
    pub cost_cents: Option<u64>,
    pub signature_verified: bool,
    pub can_revert: bool,
}

pub struct JournalView;

impl JournalView {
    pub fn format_entry(entry: &AuditEntry) -> JournalViewItem {
        let conf = (entry.confidence * 100.0).clamp(0.0, 100.0) as u8;
        let reason_summary = entry
            .reasoning_trace
            .steps
            .first()
            .map(|s| s.description.clone())
            .unwrap_or_else(|| "No trace recorded".to_string());

        JournalViewItem {
            entry_id: entry.id.clone(),
            timestamp_str: entry.timestamp.0.to_rfc3339(),
            actor_name: format!("{:?}", entry.actor),
            action_summary: format!("{}: {}", entry.action.action_type, entry.action.description),
            reasoning_summary: reason_summary,
            confidence_percentage: conf,
            cost_cents: entry.cost.as_ref().map(|c| c.amount_cents),
            signature_verified: true,
            can_revert: entry.action.reversible,
        }
    }

    pub fn explain_action(entry: &AuditEntry) -> String {
        let mut expl = format!("Action '{}' triggered by {:?}.\n", entry.action.action_type, entry.actor);
        expl.push_str(&format!("Description: {}\n", entry.action.description));
        if !entry.reasoning_trace.steps.is_empty() {
            expl.push_str("Reasoning steps:\n");
            for (i, step) in entry.reasoning_trace.steps.iter().enumerate() {
                expl.push_str(&format!("  {}. {}\n", i + 1, step.description));
            }
        }
        expl
    }
}
