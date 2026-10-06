use annora_core::Timestamp;
use annora_memory::graph::KnowledgeGraph;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuriosityInquiry {
    pub inquiry_id: String,
    pub question_prompt: String,
    pub topic_area: String,
    pub rationale: String,
    pub generated_at: Timestamp,
    pub priority: f64,
}

/// Background Graph Curiosity Scanner (§28d).
/// Operates during idle/low-carbon windows to analyze knowledge gaps or ambiguities,
/// staging proactive questions without unprompted interruption.
pub struct GraphCuriosityScanner;

impl GraphCuriosityScanner {
    /// Scan knowledge graph and identify missing connections or follow-up opportunities.
    pub fn scan_for_curiosity_inquiries(graph: &KnowledgeGraph) -> Vec<CuriosityInquiry> {
        let mut inquiries = Vec::new();
        let triplets = graph.all_triplets();

        // 1. Check if graph has few facts (bootstrap phase)
        if triplets.len() < 5 {
            inquiries.push(CuriosityInquiry {
                inquiry_id: "inq-preferences-bootstrap".to_string(),
                question_prompt: "Quels sont tes principaux projets ou priorités en ce moment ?".to_string(),
                topic_area: "Projets & Objectifs".to_string(),
                rationale: "Densité initiale du graphe faible; apprentissage des axes de vie prioritaires".to_string(),
                generated_at: Timestamp::now(),
                priority: 0.9,
            });
        }

        // 2. Identify isolated topics or entities with only 1 link
        for t in &triplets {
            if let annora_memory::NodeValue::Text(ref text) = t.object {
                if text.to_lowercase().contains("projet") || text.to_lowercase().contains("objectif") {
                    inquiries.push(CuriosityInquiry {
                        inquiry_id: format!("inq-followup-{}", t.id.0),
                        question_prompt: format!("Tu avais mentionné '{}' : y a-t-il une échéance particulière à suivre ?", text),
                        topic_area: "Planification".to_string(),
                        rationale: "Objectif détecté sans date limite explicite liée dans le graphe".to_string(),
                        generated_at: Timestamp::now(),
                        priority: 0.75,
                    });
                    break;
                }
            }
        }

        inquiries
    }
}
