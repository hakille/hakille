use hakille_core::Timestamp;
use hakille_memory::graph::KnowledgeGraph;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuriosityInquiry {
    pub inquiry_id: String,
    pub question_prompt: String,
    pub topic_area: String,
    pub rationale: String,
    pub generated_at: Timestamp,
    pub priority: f64,
}

/// Proposition proactive prête pour l'initiative bornée (§28d) : contenu
/// RÉEL de la question stagée (jamais de placeholder). Le daemon la rejoue
/// vers `BoundedInitiativeTracker::record_proposal(proposal_id, within_bounds,
/// user_accepted=false)` et le SilenceBudget (quota 3/jour, spec §18/§21d) :
/// le kernel ne fait que PROPOSER, il ne décide jamais de parler.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InitiativeProposal {
    /// Identifiant de déduplication (repris de l'`inquiry_id` stagé).
    pub proposal_id: String,
    /// Question proposée à l'utilisateur, texte intégral.
    pub question_prompt: String,
    pub topic_area: String,
    pub rationale: String,
    pub priority: f64,
    pub generated_at: Timestamp,
    /// Toujours `true` pour une question de curiosité : relance en LECTURE
    /// seule, jamais une action exécutable (miroir `within_bounds`).
    pub within_bounds: bool,
}

impl From<CuriosityInquiry> for InitiativeProposal {
    fn from(inq: CuriosityInquiry) -> Self {
        Self {
            proposal_id: inq.inquiry_id,
            question_prompt: inq.question_prompt,
            topic_area: inq.topic_area,
            rationale: inq.rationale,
            priority: inq.priority,
            generated_at: inq.generated_at,
            within_bounds: true,
        }
    }
}

/// Background Graph Curiosity Scanner (§28d).
/// Operates during idle/low-carbon windows to analyze knowledge gaps or ambiguities,
/// staging proactive questions without unprompted interruption.
/// Heuristique honnête : détection par règles (densité graphe, mot-clé
/// projet/objectif), PAS un modèle de curiosité — seuils documentés.
pub struct GraphCuriosityScanner {
    /// IDs déjà posées (déduplication) — évite de reposer la même question.
    asked_ids: HashSet<String>,
    /// File d'attente des questions mises en scène, en attente de fenêtre idle.
    staged: Vec<CuriosityInquiry>,
}

impl Default for GraphCuriosityScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphCuriosityScanner {
    pub fn new() -> Self {
        Self {
            asked_ids: HashSet::new(),
            staged: Vec::new(),
        }
    }

    /// Fenêtre idle : scan autorisé seulement si `idle_secs > 300` (5 min).
    /// Heuristique anti-interruption, pas de détection d'activité système réelle.
    pub fn should_scan(idle_secs: u64) -> bool {
        idle_secs > 300
    }

    /// Questions déjà posées (déduplication).
    pub fn asked_ids(&self) -> &HashSet<String> {
        &self.asked_ids
    }

    /// File de staging actuelle.
    pub fn staged(&self) -> &[CuriosityInquiry] {
        &self.staged
    }

    /// Marque une question comme posée (alimente la déduplication).
    pub fn mark_asked(&mut self, inquiry_id: impl Into<String>) {
        self.asked_ids.insert(inquiry_id.into());
    }

    /// Scan + déduplication + staging : appelle le scan pur statique, filtre
    /// les IDs déjà posées, pousse les nouvelles dans `staged` et les retourne.
    /// Ne pose jamais de question directement (staging uniquement).
    pub fn scan_and_stage(&mut self, graph: &KnowledgeGraph) -> Vec<CuriosityInquiry> {
        let scanned = Self::scan_for_curiosity_inquiries(graph);
        self.stage_inquiries(scanned)
    }

    /// Staging dédupliqué de questions déjà construites : filtre les IDs déjà
    /// posées (`asked_ids`), pousse les nouvelles dans `staged` (sans doublon
    /// de file) et les marque comme posées. Retourne les questions fraîchement
    /// stagées. Support du scan combiné de l'orchestrateur (densité/objectif
    /// + orphelins) avec la même déduplication que [`Self::scan_and_stage`].
    pub fn stage_inquiries(&mut self, inquiries: Vec<CuriosityInquiry>) -> Vec<CuriosityInquiry> {
        let mut fresh = Vec::new();
        for inq in inquiries {
            if self.asked_ids.contains(&inq.inquiry_id) {
                continue;
            }
            self.asked_ids.insert(inq.inquiry_id.clone());
            // Évite les doublons dans le staging lui-même.
            if !self.staged.iter().any(|s| s.inquiry_id == inq.inquiry_id) {
                self.staged.push(inq.clone());
            }
            fresh.push(inq);
        }
        fresh
    }

    /// Draine la file de staging (consommation par l'orchestrateur en fenêtre idle).
    pub fn drain_staged(&mut self) -> Vec<CuriosityInquiry> {
        std::mem::take(&mut self.staged)
    }

    /// Draine au plus `max` questions en tête du staging (reliquat conservé,
    /// ordre stable) — borne anti-flood de l'initiative bornée. `max == 0` ne
    /// draine rien.
    pub fn drain_staged_upto(&mut self, max: usize) -> Vec<CuriosityInquiry> {
        let take = max.min(self.staged.len());
        self.staged.drain(..take).collect()
    }

    /// Draine le staging vers l'initiative bornée : pour chaque question en
    /// file, appelle `record(proposal_id, within_bounds, user_accepted)`.
    /// `within_bounds` vaut toujours `true` ici (questions de relance lues
    /// seules, jamais d'action) ; `user_accepted=false` tant que l'utilisateur
    /// n'a pas validé. Retourne le nombre de propositions enregistrées.
    /// Le câblage effectif vers `BoundedInitiativeTracker::record_proposal`
    /// se fait côté orchestrateur (même signature) sans dépendance croisée.
    pub fn drain_staged_for_initiative(
        &mut self,
        record: &mut dyn FnMut(String, bool, bool),
    ) -> usize {
        let staged = self.drain_staged();
        let n = staged.len();
        for inq in staged {
            record(inq.inquiry_id, true, false);
        }
        n
    }

    /// Scan knowledge graph and identify missing connections or follow-up opportunities.
    pub fn scan_for_curiosity_inquiries(graph: &KnowledgeGraph) -> Vec<CuriosityInquiry> {
        let mut inquiries = Vec::new();
        let triplets = graph.all_triplets();

        // 1. Check if graph has few facts (bootstrap phase)
        if triplets.len() < 5 {
            inquiries.push(CuriosityInquiry {
                inquiry_id: "inq-preferences-bootstrap".to_string(),
                question_prompt: "Quels sont tes principaux projets ou priorités en ce moment ?"
                    .to_string(),
                topic_area: "Projets & Objectifs".to_string(),
                rationale:
                    "Densité initiale du graphe faible; apprentissage des axes de vie prioritaires"
                        .to_string(),
                generated_at: Timestamp::now(),
                priority: 0.9,
            });
        }

        // 2. Identify isolated topics or entities with only 1 link
        for t in &triplets {
            if let hakille_memory::NodeValue::Text(ref text) = t.object
                && (text.to_lowercase().contains("projet")
                    || text.to_lowercase().contains("objectif"))
            {
                inquiries.push(CuriosityInquiry {
                    inquiry_id: format!("inq-followup-{}", t.id.0),
                    question_prompt: format!(
                        "Tu avais mentionné '{}' : y a-t-il une échéance particulière à suivre ?",
                        text
                    ),
                    topic_area: "Planification".to_string(),
                    rationale: "Objectif détecté sans date limite explicite liée dans le graphe"
                        .to_string(),
                    generated_at: Timestamp::now(),
                    priority: 0.75,
                });
                break;
            }
        }

        inquiries
    }

    /// Mode `using_graph_orphans` (§28d) : scanne les sujets du graphe qui
    /// n'ont fait l'objet d'aucune question récente et propose une relance
    /// ciblée par sujet orphelin (max 3, triés par libellé pour déterminisme).
    /// `recent_topics` : libellés ou mots-clés déjà abordés (insensible à la
    /// casse, sous-chaîne dans les deux sens). Ne modifie pas le graphe.
    pub fn using_graph_orphans(
        graph: &KnowledgeGraph,
        recent_topics: &[String],
    ) -> Vec<CuriosityInquiry> {
        let mut labels: Vec<String> = Vec::new();
        for t in graph.all_triplets() {
            if let Some(label) = graph.subject_label(&t.subject)
                && !labels.iter().any(|l| l == label)
            {
                labels.push(label.to_string());
            }
        }
        labels.sort();
        let recent: Vec<String> = recent_topics.iter().map(|s| s.to_lowercase()).collect();
        let mut inquiries = Vec::new();
        for label in labels {
            let lower = label.to_lowercase();
            let covered = recent
                .iter()
                .any(|r| lower.contains(r.as_str()) || r.contains(lower.as_str()));
            if covered {
                continue;
            }
            inquiries.push(CuriosityInquiry {
                inquiry_id: format!("inq-orphan-{}", lower.replace(' ', "-")),
                question_prompt: format!(
                    "Tu avais mentionné '{}' : veux-tu faire le point dessus ?",
                    label
                ),
                topic_area: "Suivi orphelin".to_string(),
                rationale: "Sujet du graphe sans question récente ; relance proactive bornée"
                    .to_string(),
                generated_at: Timestamp::now(),
                priority: 0.6,
            });
            if inquiries.len() >= 3 {
                break;
            }
        }
        inquiries
    }
}
