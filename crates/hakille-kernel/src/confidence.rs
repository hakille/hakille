use hakille_core::{ConfidenceDecision, ConfidenceLevel, Domain, DomainThresholds};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedAction {
    pub action_name: String,
    pub domain: Domain,
    pub model_confidence: f64,
    pub parameters_summary: String,
}

pub struct ConfidenceEngine {
    thresholds: DomainThresholds,
    auto_act_margin: f64,
}

impl ConfidenceEngine {
    pub fn new(thresholds: DomainThresholds, auto_act_margin: f64) -> Self {
        Self {
            thresholds,
            auto_act_margin,
        }
    }

    pub fn with_default_thresholds() -> Self {
        Self {
            thresholds: DomainThresholds::default(),
            auto_act_margin: 0.05,
        }
    }

    /// Heuristique honnête d'inférence de domaine (mots-clés, PAS un
    /// classifieur) : financier > médical > légal > technique > créatif >
    /// général. Ordre = criticité décroissante : en cas de chevauchement
    /// on retient le domaine au seuil le plus strict (sûreté > rappel).
    pub fn infer_domain(prompt: &str) -> Domain {
        let lower = prompt.to_lowercase();
        let financial = [
            "paiement", "payment", "virement", "payer", "wallet", "eur", "€", "facture", "argent",
            "solde",
        ];
        if financial.iter().any(|k| lower.contains(k)) {
            return Domain::Financial;
        }
        let medical = [
            "diagnostic",
            "traitement",
            "médical",
            "medical",
            "symptôme",
            "ordonnance",
            "docteur",
        ];
        if medical.iter().any(|k| lower.contains(k)) {
            return Domain::Medical;
        }
        let legal = [
            "contrat",
            "contract",
            "juridique",
            "legal",
            "loi",
            "tribunal",
            "clause",
        ];
        if legal.iter().any(|k| lower.contains(k)) {
            return Domain::Legal;
        }
        let technical = [
            "refactor",
            "architecture",
            "code",
            "kernel",
            "preuve",
            "formel",
            "api",
            "système",
        ];
        if technical.iter().any(|k| lower.contains(k)) {
            return Domain::Technical;
        }
        let creative = [
            "histoire", "poème", "poeme", "roman", "story", "dessine", "imagine",
        ];
        if creative.iter().any(|k| lower.contains(k)) {
            return Domain::Creative;
        }
        Domain::General
    }

    /// Corroboration graphe honnête : 0.5 sans fait (neutre), +0.5·(n/3)
    /// plafonné à 1.0 pour 3+ faits. Même formule que System2 pour cohérence.
    pub fn corroboration_from_facts(facts: &[String]) -> f64 {
        if facts.is_empty() {
            0.5
        } else {
            ((facts.len().min(3) as f64 / 3.0) * 0.5 + 0.5).clamp(0.0, 1.0)
        }
    }

    /// Seuil par domaine (jamais de 0.85/0.9 en dur côté appelant) :
    /// lit `DomainThresholds` pour le domaine inféré du prompt.
    pub fn threshold_for(&self, domain: &Domain) -> f64 {
        *self.thresholds.0.get(domain).unwrap_or(&0.7)
    }

    pub fn evaluate(
        &self,
        action: &ProposedAction,
        graph_corroboration: f64,
    ) -> (ConfidenceDecision, ConfidenceLevel) {
        let level = ConfidenceLevel::compute(action.model_confidence, graph_corroboration);
        let base_threshold = *self.thresholds.0.get(&action.domain).unwrap_or(&0.7);

        let decision = if level.combined >= base_threshold + self.auto_act_margin {
            ConfidenceDecision::AutoExecute
        } else if level.combined >= base_threshold * 0.75 {
            // Borderline zone: triggers internal self-critique/contradiction check
            ConfidenceDecision::InternalContradiction
        } else if level.combined >= base_threshold * 0.5 {
            ConfidenceDecision::ProposeAndWait
        } else {
            ConfidenceDecision::Refuse
        };

        (decision, level)
    }
}
