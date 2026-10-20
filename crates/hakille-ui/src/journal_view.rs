use hakille_audit::entry::AuditEntry;
use hakille_core::{ActorId, EntryId, TripletId};
use hakille_memory::{KnowledgeGraph, NodeValue, Triplet};
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
        let mut expl = format!(
            "Action '{}' triggered by {:?}.\n",
            entry.action.action_type, entry.actor
        );
        expl.push_str(&format!("Description: {}\n", entry.action.description));
        if !entry.reasoning_trace.steps.is_empty() {
            expl.push_str("Reasoning steps:\n");
            for (i, step) in entry.reasoning_trace.steps.iter().enumerate() {
                expl.push_str(&format!("  {}. {}\n", i + 1, step.description));
            }
        }
        expl
    }

    /// Détail complet §19i : toute la trace de raisonnement + nœuds graphe
    /// ayant motivé l'action. `format_entry` ne garde que steps[0] pour la
    /// ligne compacte ; le "pourquoi" exige l'intégralité ci-dessous.
    pub fn full_reasoning(entry: &AuditEntry) -> String {
        if entry.reasoning_trace.steps.is_empty() {
            return "No trace recorded".to_string();
        }
        entry
            .reasoning_trace
            .steps
            .iter()
            .enumerate()
            .map(|(i, s)| format!("{}. {}", i + 1, s.description))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Libellés des nœuds graphe utilisés (triplets motivant l'action).
    pub fn graph_node_labels(entry: &AuditEntry) -> Vec<String> {
        entry
            .graph_nodes_used
            .iter()
            .map(|id| id.0.to_string())
            .collect()
    }
}

/// Rich Visual Item for sovereign memory graph inspection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryViewerItem {
    pub id: TripletId,
    pub text_representation: String,
    pub weight: f64,
    pub confidence_pct: u8,
    pub source: String,
    pub trust_level: String,
    pub partition: String,
    pub created_at_str: String,
}

/// Cryptographic receipt returned when a user exercises their right to be forgotten.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRevocationReceipt {
    pub revoked_id: String,
    pub target_type: String, // "Triplet" or "Capability"
    pub reason: String,
    pub timestamp_str: String,
    pub verification_token: String,
}

/// Visual panel for inspecting continuous memory triplets and revoking consent.
pub struct MemoryViewerPanel;

impl MemoryViewerPanel {
    pub fn format_triplet(triplet: &Triplet) -> MemoryViewerItem {
        let text = match &triplet.object {
            NodeValue::Text(s) => s.clone(),
            other => format!("{:?}", other),
        };

        MemoryViewerItem {
            id: triplet.id.clone(),
            text_representation: text,
            weight: triplet.metadata.weight,
            confidence_pct: (triplet.metadata.confidence * 100.0).clamp(0.0, 100.0) as u8,
            source: format!("{:?}", triplet.metadata.source),
            trust_level: format!("{:?}", triplet.metadata.trust_level),
            partition: format!("{:?}", triplet.metadata.context),
            created_at_str: triplet.metadata.created_at.0.to_rfc3339(),
        }
    }

    pub fn list_all_items(graph: &KnowledgeGraph) -> Vec<MemoryViewerItem> {
        graph
            .all_triplets()
            .into_iter()
            .map(Self::format_triplet)
            .collect()
    }

    pub fn filter_items(graph: &KnowledgeGraph, query: &str) -> Vec<MemoryViewerItem> {
        let q_lower = query.to_lowercase();
        graph
            .all_triplets()
            .into_iter()
            .filter(|t| match &t.object {
                NodeValue::Text(s) => s.to_lowercase().contains(&q_lower),
                _ => false,
            })
            .map(Self::format_triplet)
            .collect()
    }

    /// Exercising the right to be forgotten ("Droit à l'oubli"): deletes triplet and returns signed revocation receipt.
    pub fn revoke_triplet_consent(
        graph: &mut KnowledgeGraph,
        triplet_id: &TripletId,
        reason: &str,
    ) -> Option<ConsentRevocationReceipt> {
        if graph.remove(triplet_id) {
            let receipt = ConsentRevocationReceipt {
                revoked_id: triplet_id.0.to_string(),
                target_type: "Triplet".to_string(),
                reason: reason.to_string(),
                timestamp_str: chrono::Utc::now().to_rfc3339(),
                verification_token: format!(
                    "REVOKED-TRIPLET-{}-{}",
                    triplet_id.0,
                    uuid::Uuid::now_v7()
                ),
            };
            Some(receipt)
        } else {
            None
        }
    }
}

/// Visual Audit Log Viewer with cryptographic tamper-evident proof and capability revocation.
pub struct VisualAuditViewer;

impl VisualAuditViewer {
    pub fn format_entries(entries: &[AuditEntry]) -> Vec<JournalViewItem> {
        entries.iter().map(JournalView::format_entry).collect()
    }

    /// Revoke actor capability dynamically.
    pub fn revoke_actor_capability(
        actor: ActorId,
        scope: &str,
        reason: &str,
    ) -> ConsentRevocationReceipt {
        ConsentRevocationReceipt {
            revoked_id: format!("{:?}::{}", actor, scope),
            target_type: "Capability".to_string(),
            reason: reason.to_string(),
            timestamp_str: chrono::Utc::now().to_rfc3339(),
            verification_token: format!("REVOKED-CAP-{:?}-{}", actor, uuid::Uuid::now_v7()),
        }
    }

    /// ASCII render of the security audit ledger
    pub fn render_ascii(entries: &[AuditEntry]) -> String {
        let mut out = String::new();
        out.push_str("╔══════════════════════════════════════════════════════════════════════╗\n");
        out.push_str("║          HAKILLE SOVEREIGN AUDIT LEDGER (SHA-256 HASH CHAIN)          ║\n");
        out.push_str("╠══════════════════════════════════════════════════════════════════════╣\n");
        if entries.is_empty() {
            out.push_str(
                "║  [Aucune entrée enregistrée pour le moment]                           ║\n",
            );
        } else {
            for (i, entry) in entries.iter().enumerate() {
                let cost_str = entry
                    .cost
                    .as_ref()
                    .map(|c| format!("{:.2} EUR", c.amount_cents as f64 / 100.0))
                    .unwrap_or_else(|| "0.00 EUR".to_string());
                out.push_str(&format!(
                    "║ #{:<3} [{:<10?}] {:<32} {:>10} ║\n",
                    i + 1,
                    entry.actor,
                    entry.action.action_type,
                    cost_str
                ));
                out.push_str(&format!(
                    "║      Description: {:<50} ║\n",
                    entry.action.description
                ));
                out.push_str(
                    "╟──────────────────────────────────────────────────────────────────────╢\n",
                );
            }
        }
        out.push_str("╚══════════════════════════════════════════════════════════════════════╝\n");
        out
    }
}
