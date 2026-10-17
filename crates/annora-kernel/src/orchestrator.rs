use crate::budget::{LatencyTracker, PipelineBudgetConfig};
use crate::confidence::{ConfidenceEngine, ProposedAction};
use crate::context::InteractionContext;
use crate::contradiction::{ContradictionAnalysis, InternalContradictionEngine};
use crate::curiosity::GraphCuriosityScanner;
use crate::fabric::CognitiveFabric;
use crate::planner::{DeliberatePlanner, ExecutionPlan, PlanReflection};
use crate::priority::{Directive, PriorityEngine, PriorityLevel};
use crate::router::{CognitiveInput, CognitiveRouter, RoutingDecision};
use crate::self_model::{CognitiveState, SelfModel};
use crate::simulator::{DryRunReport, DryRunSimulator};
use crate::system1::System1Engine;
use crate::system2::System2Engine;
use crate::working_memory::WorkingMemory;
use annora_constitution::rules::Constitution;
use annora_core::{
    AnnoraError, AuthorizationContext, CapabilityAction, CapabilityAuthority, CapabilityScope,
    CapabilityToken, ConfidenceDecision, Domain, Result, TrustLevel, UserId,
};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorResult {
    pub response_text: String,
    pub routing_used: String,
    pub latency_ms: u64,
    pub confidence_decision: ConfidenceDecision,
    pub estimated_cost_cents: Option<u64>,
}

pub struct CognitiveOrchestrator {
    pub fabric: CognitiveFabric,
    pub router: CognitiveRouter,
    pub system1: System1Engine,
    pub system2: System2Engine,
    pub context: InteractionContext,
    pub confidence_engine: ConfidenceEngine,
    pub contradiction_engine: InternalContradictionEngine,
    pub priority_engine: PriorityEngine,
    pub constitution: Constitution,
    pub memory_graph: annora_memory::KnowledgeGraph,
    pub embedding_engine: Option<annora_inference::MiniLmOnnx>,
    pub budget_config: PipelineBudgetConfig,
    /// Mémoire de travail éphémère (scratchpad, §28a) — jamais persistée telle
    /// quelle, distillée vers le graphe souverain via `distill_and_clear`.
    pub working_memory: WorkingMemory,
    /// Modèle introspectif temps réel (§28c) — transitions
    /// Perceiving → Reasoning → Executing exposées via `current_activity()`.
    pub self_model: SelfModel,
    /// Scanner de curiosité avec déduplication + staging (§28d).
    pub curiosity_scanner: GraphCuriosityScanner,
    /// Dernière réflexion Plan-Execute-Reflect (après S2), pour audit.
    pub last_reflection: Option<PlanReflection>,
    /// Dernière simulation dry-run avant action irréversible, pour audit.
    pub last_simulation: Option<DryRunReport>,
    /// Dernière analyse de contradiction interne (zone limite), pour audit.
    pub last_contradiction: Option<ContradictionAnalysis>,
    /// Contradictions en attente de l'utilisateur (paires existant/entrant,
    /// jamais écrasées) — mémoire bout-en-bout : `remember` y pousse (avec
    /// `tracing::warn`) au lieu d'ignorer `NeedsUserResolution` en silence.
    pub pending_resolutions: Vec<(annora_memory::Triplet, annora_memory::Triplet)>,
    /// Initiative bornée (miroir de `BoundedInitiativeTracker::record_proposal`) :
    /// `(proposal_id, within_bounds, user_accepted)`. Alimenté par
    /// `drain_staged_for_initiative` après chaque tour — sans dépendance
    /// croisée vers `annora-metrics` (le daemon rejoue vers le tracker).
    pub initiative_log: Vec<(String, bool, bool)>,
    /// Continuité (miroir de `ContinuityTracker::record_task`) :
    /// `(task_id, required_re_explanation, turns_count)`.
    pub continuity_log: Vec<(String, bool, usize)>,
    /// Généralité (miroir de `GeneralityTracker::record_task`) : domaine
    /// inféré par `ConfidenceEngine::infer_domain` à chaque tour.
    pub generality_log: Vec<Domain>,
}

impl CognitiveOrchestrator {
    pub fn new(fabric: CognitiveFabric, user_id: UserId, constitution: Constitution) -> Self {
        let wm_task = format!("session-{}", user_id.0);
        Self {
            fabric,
            router: CognitiveRouter::with_default_classifier(),
            system1: System1Engine::new(100),
            system2: System2Engine::new("DefaultSystem2Slot", 2),
            context: InteractionContext::new(user_id, 100),
            confidence_engine: ConfidenceEngine::with_default_thresholds(),
            contradiction_engine: InternalContradictionEngine::new(),
            priority_engine: PriorityEngine::new(),
            constitution,
            memory_graph: annora_memory::KnowledgeGraph::new(),
            embedding_engine: None,
            budget_config: PipelineBudgetConfig::default(),
            working_memory: WorkingMemory::new(wm_task),
            self_model: SelfModel::new(),
            curiosity_scanner: GraphCuriosityScanner::new(),
            last_reflection: None,
            last_simulation: None,
            last_contradiction: None,
            pending_resolutions: Vec::new(),
            initiative_log: Vec::new(),
            continuity_log: Vec::new(),
            generality_log: Vec::new(),
        }
    }

    /// Introspection exposée (§28c) : "Qu'est-ce que tu es en train de faire ?"
    pub fn current_activity(&self) -> String {
        self.self_model.query_current_activity()
    }

    /// Heuristique honnête d'inférence du type d'action (mots-clés, PAS un
    /// classifieur) : détecte les intentions irréversibles pour déclencher le
    /// dry-run. Retourne `WalletPayment` / `FileDelete` / `FileWrite` /
    /// `ReplyToUser` (réversible, pas de simulation).
    pub fn infer_action_type(prompt: &str) -> String {
        let lower = prompt.to_lowercase();
        // Paiement : virement / payer / montant en EUR.
        if lower.contains("paiement")
            || lower.contains("payment")
            || lower.contains("virement")
            || lower.contains("payer")
            || lower.contains("paye ")
        {
            return "WalletPayment".to_string();
        }
        // Suppression : irréversible, simulation obligatoire.
        if lower.contains("supprim")
            || lower.contains("delete")
            || lower.contains("efface")
            || lower.contains("détruis")
        {
            return "FileDelete".to_string();
        }
        // Écriture fichier : potentiellement destructive.
        if lower.contains("fichier")
            || lower.contains("file")
            || lower.contains("écris")
            || lower.contains("enregistre")
        {
            return "FileWrite".to_string();
        }
        "ReplyToUser".to_string()
    }

    /// Dry-run avant action irréversible (§24d) : si `action_type` contient
    /// `payment`/`file`/`delete` (insensible à la casse), simule localement et
    /// journalise via `tracing`. Heuristique : montants/chemins extraits par
    /// défaut (1000 cents, solde 10000, `/tmp/annora-dryrun`) faute de
    /// paramètres structurés — jamais une exécution réelle.
    pub fn dry_run_for_action(&self, action_type: &str, prompt: &str) -> Option<DryRunReport> {
        let lowered = action_type.to_lowercase();
        if lowered.contains("payment") {
            // Montant heuristique : premier nombre suivi de €/EUR, défaut 10.00 EUR.
            let amount_cents = Self::extract_amount_cents(prompt).unwrap_or(1000);
            let report = DryRunSimulator::simulate_payment(
                "destinataire-inconnu",
                amount_cents,
                "EUR",
                10_000,
            );
            tracing::info!(
                action_type = %action_type,
                preview = %report.human_readable_preview,
                "dry-run paiement avant engagement irréversible"
            );
            return Some(report);
        }
        if lowered.contains("delete") {
            let path =
                Self::extract_path(prompt).unwrap_or_else(|| "/tmp/annora-dryrun".to_string());
            let report = DryRunSimulator::simulate_file_mutation(&path, true, 2048, 0);
            tracing::info!(
                action_type = %action_type,
                preview = %report.human_readable_preview,
                "dry-run suppression avant engagement irréversible"
            );
            return Some(report);
        }
        if lowered.contains("file") {
            let path =
                Self::extract_path(prompt).unwrap_or_else(|| "/tmp/annora-dryrun".to_string());
            let report = DryRunSimulator::simulate_file_mutation(&path, false, 2048, 2048);
            tracing::info!(
                action_type = %action_type,
                preview = %report.human_readable_preview,
                "dry-run écriture fichier avant engagement"
            );
            return Some(report);
        }
        None
    }

    /// Extrait un montant heuristique en centimes (ex: "25€", "25.50 EUR").
    /// Retourne `None` si aucun nombre détecté — l'appelant applique le défaut.
    fn extract_amount_cents(prompt: &str) -> Option<u64> {
        let mut num = String::new();
        let mut found = false;
        for ch in prompt.chars() {
            if ch.is_ascii_digit() || ch == '.' || ch == ',' {
                num.push(if ch == ',' { '.' } else { ch });
                found = true;
            } else if found {
                break;
            }
        }
        if num.is_empty() {
            return None;
        }
        num.parse::<f64>().ok().map(|euros| (euros * 100.0) as u64)
    }

    /// Extrait un chemin heuristique (premier token contenant `/`).
    fn extract_path(prompt: &str) -> Option<String> {
        prompt
            .split_whitespace()
            .find(|w| w.contains('/'))
            .map(|s| {
                s.trim_matches(|c: char| {
                    !c.is_alphanumeric() && c != '/' && c != '.' && c != '_' && c != '-'
                })
                .to_string()
            })
            .filter(|s| !s.is_empty())
    }

    /// Tokens minuscules à mot exact (anti faux positifs : « pays »
    /// (nation) ≠ « pay », « nom » ≠ « non », « regarde » ≠ « garde »).
    fn prompt_tokens(prompt: &str) -> Vec<String> {
        prompt
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .map(|t| t.to_string())
            .collect()
    }

    /// Porte dry-run (mission plan) : verbes irréversibles FR/EN à mot exact.
    /// Périmètre : payer/supprimer/envoyer/transférer + pay/delete/send/
    /// transfer/unlock (+ conjugaisons FR, formes fléchies EN, « virement »
    /// quasi-systématiquement actionnel). « paiement » seul est exclu
    /// (simple évocation) — l'intention de payer passe par le verbe.
    pub fn is_irreversible_prompt(prompt: &str) -> bool {
        const TRIGGERS: &[&str] = &[
            "payer",
            "paie",
            "paies",
            "paye",
            "payes",
            "payons",
            "payez",
            "paient",
            "supprimer",
            "supprime",
            "supprimes",
            "suppriment",
            "supprimez",
            "supprimons",
            "suppression",
            "suppressions",
            "effacer",
            "efface",
            "effaces",
            "effacent",
            "effacez",
            "détruire",
            "détruis",
            "détruit",
            "détruisent",
            "détruisez",
            "envoyer",
            "envoie",
            "envoies",
            "envoient",
            "envoyez",
            "envoyons",
            "envoi",
            "envois",
            "transférer",
            "transferer",
            "transfère",
            "transfere",
            "transfères",
            "transferes",
            "transfèrent",
            "transferent",
            "transférez",
            "transferez",
            "transfert",
            "transferts",
            "débloquer",
            "debloquer",
            "débloque",
            "debloque",
            "débloques",
            "virement",
            "virements",
            "pay",
            "payment",
            "payments",
            "delete",
            "deletes",
            "send",
            "sends",
            "transfer",
            "transfers",
            "unlock",
            "unlocks",
        ];
        let tokens = Self::prompt_tokens(prompt);
        tokens.iter().any(|t| TRIGGERS.contains(&t.as_str()))
    }

    /// Carte ShowCard-like pour action irréversible : construit
    /// `ExecutionPlan::new()` + 1 pas irréversible (risque auto-détecté ⇒
    /// `requires_human_confirmation`), simule (`simulate_payment` si
    /// wallet/montant, sinon `simulate_file_mutation`), et rend le texte
    /// `CARTE REQUISE` + pastilles coût/données/réversibilité. Zéro exécution.
    /// Enforcement capabilities : voir [`Self::build_plan_card_authorized`] +
    /// [`Self::verify_plan_against_authority`] (contexte validé, token expiré
    /// refusé, `authority.verify_pinned` + couverture scope par pas). Le chemin
    /// sans capabilities ci-dessous reste carte + `requires_human_confirmation`
    /// (jamais d'exécution silencieuse) ; le chemin autorisé vérifie avant
    /// de cartographier.
    fn build_plan_card(task_id: &str, prompt: &str) -> (ExecutionPlan, DryRunReport, String) {
        let goal: String = prompt.chars().take(140).collect();
        let tokens = Self::prompt_tokens(prompt);
        const MONEY: &[&str] = &[
            "eur",
            "€",
            "euro",
            "euros",
            "usd",
            "dollar",
            "dollars",
            "$",
            "centime",
            "centimes",
            "montant",
            "virement",
            "virements",
            "paiement",
            "payment",
        ];
        const PAY_VERBS: &[&str] = &[
            "payer",
            "paie",
            "paies",
            "paye",
            "payes",
            "payons",
            "payez",
            "transférer",
            "transferer",
            "transfère",
            "transfere",
            "transfert",
            "pay",
            "payment",
            "transfer",
        ];
        let is_payment = tokens.iter().any(|t| MONEY.contains(&t.as_str()))
            || tokens.iter().any(|t| PAY_VERBS.contains(&t.as_str()));
        let mut plan = ExecutionPlan::new(task_id, goal.clone());
        let report = if is_payment {
            plan.add_step(
                format!("Virement : {}", goal),
                CapabilityScope::Wallet,
                CapabilityAction::Execute,
                true,
            );
            let amount_cents = Self::extract_amount_cents(prompt).unwrap_or(1000);
            DryRunSimulator::simulate_payment("bénéficiaire", amount_cents, "EUR", 10_000)
        } else {
            const DEL_SIDE: &[&str] = &[
                "supprimer",
                "supprime",
                "supprimes",
                "suppriment",
                "suppression",
                "effacer",
                "efface",
                "effaces",
                "détruire",
                "détruit",
                "delete",
                "deletes",
            ];
            let is_deletion = tokens.iter().any(|t| DEL_SIDE.contains(&t.as_str()));
            let target =
                Self::extract_path(prompt).unwrap_or_else(|| "<cible non précisée>".to_string());
            plan.add_step(
                format!(
                    "{} : {} sur {}",
                    if is_deletion {
                        "Suppression"
                    } else {
                        "Écriture"
                    },
                    goal,
                    target
                ),
                CapabilityScope::Filesystem,
                if is_deletion {
                    CapabilityAction::Delete
                } else {
                    CapabilityAction::Write
                },
                true,
            );
            DryRunSimulator::simulate_file_mutation(&target, is_deletion, 2048, 0)
        };
        let warning = report.warning.clone().unwrap_or_else(|| {
            "Verrou plan : confirmation humaine requise (pas irréversible).".to_string()
        });
        let (cost_pastille, data_pastille) = if report.action_type == "WalletPayment" {
            (
                format!(
                    "montant simulé, solde restant estimé {}",
                    report
                        .diffs
                        .first()
                        .map(|d| d.state_after.as_str())
                        .unwrap_or("?")
                ),
                "montant + destinataire".to_string(),
            )
        } else {
            (
                "0 — aucun transfert".to_string(),
                format!(
                    "chemin {}",
                    report
                        .diffs
                        .first()
                        .map(|d| d.target_resource.as_str())
                        .unwrap_or("?")
                ),
            )
        };
        let card = format!(
            "CARTE REQUISE — action irréversible détectée : « {} »\n  • Plan : {} ({} étape(s), risque {:?}) — AUCUNE exécution.\n  • Simulation : {}\n  • Avertissement : {}\n  • Pastilles : [coût : {}] [données : {}] [réversibilité : NON — confirmation humaine explicite requise]\nConfirmez explicitement pour continuer, ou abandonnez.",
            goal,
            plan.plan_id,
            plan.steps.len(),
            plan.estimated_risk,
            report.human_readable_preview,
            warning,
            cost_pastille,
            data_pastille
        );
        (plan, report, card)
    }

    /// Audience attendue par l'orchestrateur pour les contextes d'autorisation.
    pub const EXPECTED_AUDIENCE: &'static str = "annora-kernel";

    /// Enforcement pré-exécution : valide le contexte (ressource normalisée,
    /// expiry, audience, quota, nonce, délégation), refuse les tokens expirés,
    /// puis vérifie chaque pas du plan via `authority.verify_pinned`
    /// (signature épinglée + scope/action). `Err` explicite à chaque refus,
    /// jamais d'exécution partielle.
    pub fn verify_plan_against_authority(
        plan: &ExecutionPlan,
        token: &CapabilityToken,
        authority: &CapabilityAuthority,
        ctx: &AuthorizationContext,
    ) -> Result<()> {
        plan.verify_capabilities_with_context(token, authority, ctx, Self::EXPECTED_AUDIENCE)
            .map_err(|msg| AnnoraError::CapabilityDenied(msg))
    }

    /// Variante autorisée de [`Self::build_plan_card`] : construit le plan +
    /// la simulation, VÉRIFIE les capabilities via
    /// [`Self::verify_plan_against_authority`], puis rend la carte. Échec de
    /// vérification = `Err` explicite (pas de carte, pas d'exécution).
    pub fn build_plan_card_authorized(
        task_id: &str,
        prompt: &str,
        ctx: &AuthorizationContext,
        token: &CapabilityToken,
        authority: &CapabilityAuthority,
    ) -> Result<(ExecutionPlan, DryRunReport, String)> {
        let (plan, report, card) = Self::build_plan_card(task_id, prompt);
        Self::verify_plan_against_authority(&plan, token, authority, ctx)?;
        Ok((plan, report, card))
    }

    /// Aperçu sec pour le daemon (`:plan <texte>`) : carte si irréversible,
    /// sinon message explicite. Lecture seule — zéro mutation, zéro exécution.
    pub fn plan_preview(&self, prompt: &str) -> String {
        if Self::is_irreversible_prompt(prompt) {
            let (_, _, card) = Self::build_plan_card("plan-preview", prompt);
            card
        } else {
            let short: String = prompt.chars().take(120).collect();
            format!(
                "AUCUNE CARTE REQUISE : aucune action irréversible détectée dans « {} » — réponse directe autorisée.",
                short
            )
        }
    }

    /// Directive de priorité dérivée du niveau de confiance (mission priority) :
    /// User → ExplicitUserInstruction ; System → Constitution ; Guest →
    /// GeneralUserPreference ; Untrusted → SystemDefault (jamais exécutable).
    fn priority_for_trust(trust: &TrustLevel) -> PriorityLevel {
        match trust {
            TrustLevel::System => PriorityLevel::Constitution,
            TrustLevel::User => PriorityLevel::ExplicitUserInstruction,
            TrustLevel::Guest => PriorityLevel::GeneralUserPreference,
            TrustLevel::Untrusted => PriorityLevel::SystemDefault,
        }
    }

    /// Journalise le tour en file de distillation (sujet unique par tour :
    /// jamais de conflit S/P avec l'existant, auto-nettoyé par le decay).
    fn queue_turn_journal(&mut self, task_id: &str, prompt: &str) {
        let short_id: String = task_id.chars().take(8).collect();
        let object: String = prompt.chars().take(140).collect();
        self.working_memory.queue_for_distillation(
            format!("Tour {}", short_id),
            "résumé",
            object,
            0.6,
        );
    }

    /// Distillation explicite `Vec<DistilledFact>` → `remember()` (enveloppe
    /// Sense→Reflect) : mappe chaque fait distillé vers le graphe souverain.
    fn distill_to_graph(&mut self) {
        let facts = self.working_memory.distill_and_clear();
        for f in &facts {
            let _ = self.remember(&f.subject, &f.predicate, &f.object);
        }
    }

    /// Domaine de généralité du tour (miroir `GeneralityTracker::record_task`) :
    /// `ConfidenceEngine::infer_domain` (heuristique mots-clés, PAS un
    /// classifieur). Le daemon mappe vers `TaskDomain` (Financial→Planning,
    /// Technical→Coding, etc.) — ici on garde le `Domain` brut honnête.
    pub fn infer_generality_domain(prompt: &str) -> Domain {
        ConfidenceEngine::infer_domain(prompt)
    }

    /// Draine le staging curiosité vers l'initiative bornée (P3) : chaque
    /// question stagée alimente `initiative_log` via la même signature que
    /// `BoundedInitiativeTracker::record_proposal(proposal_id, within_bounds,
    /// user_accepted)`. Retourne le nombre de propositions enregistrées.
    pub fn drain_curiosity_to_initiative(&mut self) -> usize {
        let mut logged = 0usize;
        let staged = self.curiosity_scanner.drain_staged();
        for inq in staged {
            self.initiative_log.push((inq.inquiry_id, true, false));
            logged += 1;
        }
        logged
    }

    /// Enregistre continuité + généralité après chaque tour (P3) :
    /// - `ContinuityTracker::record_task(task_id, required_re_explanation,
    ///   turns_count)` : `required_re_explanation = decision != AutoExecute`
    ///   (refus/proposition/carte ⇒ l'utilisateur doit trancher), `turns=1` ;
    /// - `GeneralityTracker::record_task(domain=infer_domain(prompt))`.
    /// Appelée sur TOUS les chemins de sortie (refus, carte, réponse).
    fn record_turn_metrics(&mut self, task_id: &str, prompt: &str, decision: &ConfidenceDecision) {
        let needs_reexplain = *decision != ConfidenceDecision::AutoExecute;
        self.continuity_log
            .push((task_id.to_string(), needs_reexplain, 1));
        self.generality_log
            .push(Self::infer_generality_domain(prompt));
        let _ = self.drain_curiosity_to_initiative();
    }

    pub fn set_inference_engine(&mut self, engine: annora_inference::GgufInferenceEngine) {
        self.system1.set_inference_engine(engine.clone());
        self.system2.set_inference_engine(engine);
    }

    /// Moteur d'embeddings RÉEL (MiniLM via `ort`, WordPiece officiel).
    /// En cas d'échec (checkpoint absent), le rappel reste SPO + substring —
    /// jamais de repli hashé silencieux.
    pub fn set_embedding_engine(&mut self, engine: annora_inference::MiniLmOnnx) {
        self.embedding_engine = Some(engine);
    }

    pub fn set_budget_config(&mut self, config: PipelineBudgetConfig) {
        self.budget_config = config;
    }

    /// Mémorisation SPO réelle (mission 1+2).
    ///
    /// - Sujet/prédicat = IDs **déterministes** dérivés du texte via le
    ///   registre nom→UUID du graphe (hash stable FNV-1a, voir
    ///   `KnowledgeGraph::deterministic_uuid`). Plus de `Uuid::now_v7()`
    ///   pour sujet/prédicat : le même libellé redonne toujours le même ID,
    ///   rendant le fait requêtable par sujet/prédicat.
    /// - Objet = **3e partie du texte uniquement** (`NodeValue::Text(object)`),
    ///   jamais la concaténation « sujet prédicat objet » de l'ancien stub.
    /// - `detect_contradiction` est appelé **avant** insert : si la résolution
    ///   exige l'utilisateur, on retourne `NeedsUserResolution` **sans écraser**
    ///   (jamais silencieux) ; `KnowledgeGraph::insert` ré-applique le même
    ///   routage en défense profonde (`ContradictionResolved` si remplacement).
    pub fn remember(
        &mut self,
        subject: &str,
        predicate: &str,
        object: &str,
    ) -> annora_memory::InsertResult {
        let subject_id = self.memory_graph.resolve_subject(subject);
        let predicate_id = self.memory_graph.resolve_predicate(predicate);
        let object_value = annora_memory::NodeValue::Text(object.to_string());
        let triplet_id = annora_memory::KnowledgeGraph::deterministic_triplet_id(
            &subject_id,
            &predicate_id,
            &object_value,
        );
        let triplet = annora_memory::Triplet {
            id: triplet_id,
            subject: subject_id,
            predicate: predicate_id,
            object: object_value,
            node_type: Some(annora_memory::NodeType::Fact),
            metadata: annora_memory::TripletMetadata {
                weight: 1.0,
                created_at: annora_core::Timestamp::now(),
                last_activated: annora_core::Timestamp::now(),
                source: annora_memory::TripletSource::User,
                confidence: 1.0,
                context: annora_memory::ContextPartition::Personal,
                trust_level: annora_core::TrustLevel::User,
                decay_rate: 0.01,
            },
        };
        // Pré-contrôle exigé par la mission : jamais d'écrasement silencieux.
        for existing in self.memory_graph.all_triplets() {
            if existing.subject != triplet.subject || existing.predicate != triplet.predicate {
                continue;
            }
            if annora_memory::detect_contradiction(existing, &triplet).is_some() {
                match annora_memory::resolve(existing, &triplet) {
                    annora_memory::Resolution::AskUser { .. } => {
                        tracing::warn!(
                            "remember: contradiction exige l'utilisateur — rien écrasé, mis en attente (sujet={:?} prédicat={:?})",
                            triplet.subject,
                            triplet.predicate
                        );
                        self.pending_resolutions
                            .push((existing.clone(), triplet.clone()));
                        return annora_memory::InsertResult::NeedsUserResolution {
                            existing: existing.clone(),
                            incoming: triplet,
                        };
                    }
                    // Remplacement : on laisse `insert` l'exécuter (traçabilité
                    // ContradictionResolved) au lieu de le faire ici.
                    annora_memory::Resolution::ReplaceWithNew => break,
                    annora_memory::Resolution::KeepExisting => {}
                }
            }
        }
        self.memory_graph.insert(triplet)
    }

    fn object_to_string(object: &annora_memory::NodeValue) -> String {
        match object {
            annora_memory::NodeValue::Text(s) => s.clone(),
            other => format!("{:?}", other),
        }
    }

    fn push_unique(results: &mut Vec<String>, value: String) {
        if !results.contains(&value) {
            results.push(value);
        }
    }

    /// Rappel en 3 étages (mission 3) :
    /// 1. `GraphQuery` déclarative (query_lang) avec filtre texte, triée par
    ///    poids puis confiance décroissants ;
    /// 2. registre SPO : si la requête (ou un mot ≥ 3 lettres) nomme un sujet
    ///    ou prédicat connu, on ramène les objets liés — c'est ce qui rend un
    ///    fait SPO réel retrouvable (« nom » ⇒ « Amadou ») sans concaténation ;
    /// 3. fallback substring brut (`search_text`) si les étages typés sont vides.
    /// Dernier recours documenté : similarité cosinus artisanale à seuil 0.40
    /// sur l'embedder MiniLM de secours (projection n-grammes, PAS le vrai
    /// modèle All-MiniLM-L6-v2 — le seuil 0.40 est conservé pour compatibilité
    /// et doit être recalibré quand le vrai modèle sera branché).
    pub fn recall(&self, query: &str) -> Vec<String> {
        let mut results = Vec::new();
        let mut ranked: Vec<&annora_memory::Triplet> = annora_memory::GraphQuery::new()
            .with_text_filter(query.to_string())
            .with_limit(100)
            .execute(&self.memory_graph);
        ranked.sort_by(|a, b| {
            b.metadata
                .weight
                .partial_cmp(&a.metadata.weight)
                .unwrap_or(Ordering::Equal)
                .then(
                    b.metadata
                        .confidence
                        .partial_cmp(&a.metadata.confidence)
                        .unwrap_or(Ordering::Equal),
                )
        });
        for t in ranked {
            Self::push_unique(&mut results, Self::object_to_string(&t.object));
        }

        // Étage registre SPO : requête entière + mots significatifs.
        let mut names: Vec<String> = vec![query.to_string()];
        for word in query.split_whitespace() {
            let clean: String = word.chars().filter(|c| c.is_alphanumeric()).collect();
            if clean.len() >= 3 {
                names.push(clean);
            }
        }
        let mut spo_hits: Vec<&annora_memory::Triplet> = Vec::new();
        for name in &names {
            if let Some(sid) = self.memory_graph.lookup_subject(name) {
                for t in self.memory_graph.all_triplets() {
                    if &t.subject == &sid && !spo_hits.iter().any(|h| h.id == t.id) {
                        spo_hits.push(t);
                    }
                }
            }
            if let Some(pid) = self.memory_graph.lookup_predicate(name) {
                for t in self.memory_graph.all_triplets() {
                    if &t.predicate == &pid && !spo_hits.iter().any(|h| h.id == t.id) {
                        spo_hits.push(t);
                    }
                }
            }
        }
        spo_hits.sort_by(|a, b| {
            b.metadata
                .weight
                .partial_cmp(&a.metadata.weight)
                .unwrap_or(Ordering::Equal)
                .then(
                    b.metadata
                        .confidence
                        .partial_cmp(&a.metadata.confidence)
                        .unwrap_or(Ordering::Equal),
                )
        });
        for t in spo_hits {
            Self::push_unique(&mut results, Self::object_to_string(&t.object));
        }

        // Fallback substring brut.
        if results.is_empty() {
            for t in self.memory_graph.search_text(query) {
                Self::push_unique(&mut results, Self::object_to_string(&t.object));
            }
        }
        // Dernier recours : similarité sémantique RÉELLE (MiniLM via `ort`,
        // WordPiece officiel, mean-pooling + L2). En cas d'échec d'embedding
        // (texte vide, séquence trop longue), le triplet est ignoré avec un
        // `warn!` — jamais de similarité hashée de secours.
        if let Some(ref embedder) = self.embedding_engine {
            match embedder.embed(query) {
                Ok(q_emb) => {
                    for t in self.memory_graph.all_triplets() {
                        if let annora_memory::NodeValue::Text(ref text) = t.object {
                            if !results.contains(text) {
                                match embedder.embed(text) {
                                    Ok(t_emb) => {
                                        let sim = annora_inference::cosine_sim(&q_emb, &t_emb);
                                        if sim >= 0.40 {
                                            results.push(text.clone());
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            error = %e,
                                            "recall: embedding réel en échec, triplet ignoré"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "recall: embedding réel de la requête en échec — rappel SPO/substring seul"
                    );
                }
            }
        }
        results
    }

    pub async fn process_user_turn(
        &mut self,
        prompt: &str,
        trust_level: &TrustLevel,
    ) -> Result<OrchestratorResult> {
        self.process_user_turn_inner(prompt, trust_level, None)
            .await
    }

    /// Chemin autorisé : thread le [`AuthorizationContext`] + token Biscuit +
    /// authority dans tout le tour. Enforcement fail-closed : contexte validé
    /// (ressource, expiry, audience, quota, nonce, délégation), token expiré
    /// refusé, `authority.verify_pinned` + couverture scope par pas de plan,
    /// capability Network/Execute exigée pour tout provider Cloud (`Err`
    /// explicite sinon). Zéro exécution sans capabilities suffisantes.
    pub async fn process_user_turn_with_authorization(
        &mut self,
        prompt: &str,
        trust_level: &TrustLevel,
        ctx: &AuthorizationContext,
        token: &CapabilityToken,
        authority: &CapabilityAuthority,
    ) -> Result<OrchestratorResult> {
        self.process_user_turn_inner(prompt, trust_level, Some((ctx, token, authority)))
            .await
    }

    async fn process_user_turn_inner(
        &mut self,
        prompt: &str,
        trust_level: &TrustLevel,
        auth: Option<(
            &AuthorizationContext,
            &CapabilityToken,
            &CapabilityAuthority,
        )>,
    ) -> Result<OrchestratorResult> {
        // Gate d'entrée autorisée (fail-closed) : si un contexte est fourni,
        // il est validé + audience vérifiée + token non expiré + token couvrant
        // le scope/action du contexte AVANT tout traitement.
        if let Some((ctx, token, authority)) = auth {
            ctx.validate().map_err(|e| {
                AnnoraError::CapabilityDenied(format!(
                    "tour refusé : contexte d'autorisation invalide: {e}"
                ))
            })?;
            ctx.check_audience(Self::EXPECTED_AUDIENCE)?;
            if token.is_expired() {
                return Err(AnnoraError::CapabilityExpired);
            }
            ctx.authorize(token, authority, Self::EXPECTED_AUDIENCE)?;
        }
        // Budget pipeline : chaque étape est chronométrée via `record_stage`,
        // vérifiée per-stage via `check_stage` (max_* dédié) ET globalement via
        // `is_within_budget()`. Abort dès dépassement, jamais silencieux.
        let mut tracker = LatencyTracker::start(self.budget_config.clone());
        let mut stage_start = Instant::now();
        // Helper : enregistre + vérifie per-stage + vérifie total.
        let record_and_check = |tracker: &mut LatencyTracker,
                                stage: &'static str,
                                duration: std::time::Duration|
         -> Result<()> {
            tracker.record_stage(stage, duration);
            // Per-stage d'abord (localise le fautif), puis total.
            tracker.check_stage(stage, duration)?;
            if !tracker.is_within_budget() {
                return Err(AnnoraError::Internal(format!(
                    "Pipeline budget exceeded after stage '{}' (elapsed {} ms > max {} ms)",
                    stage,
                    tracker.total_elapsed().as_millis(),
                    tracker.config().max_total_ms,
                )));
            }
            Ok(())
        };

        // --- Enveloppe Sense→Reflect : WorkingMemory fraîche par tour ---
        // task_id = UUID v7 (traçabilité + sujet journal unique). L'enveloppe
        // précédente qui déborde (>10 notes) est d'abord distillée vers le
        // graphe souverain (seuls les faits en file sont persistés).
        let task_id = uuid::Uuid::now_v7().to_string();
        if self.working_memory.scratchpad_notes.len() > 10 {
            let distill_start = Instant::now();
            self.distill_to_graph();
            tracing::info!("working memory précédente distillée, enveloppe fraîche ouverte");
            let d = distill_start.elapsed();
            record_and_check(&mut tracker, "distill", d)?;
            stage_start = Instant::now();
        }
        self.working_memory = WorkingMemory::new(task_id.clone());
        // Transition 1/3 : Perceiving. Le résumé de tâche est le prompt tronqué
        // (heuristique, pas de compréhension) pour l'introspection.
        self.self_model.set_state(CognitiveState::Perceiving);
        self.self_model.current_task_summary = Some(prompt.chars().take(120).collect::<String>());
        // Sense : note du prompt + observation d'entrée (éphémères, pas graphe).
        let short_prompt: String = prompt.chars().take(200).collect();
        self.working_memory
            .add_note(format!("user_turn: {}", short_prompt));
        let obs_idx = self.working_memory.observations.len() as u32;
        self.working_memory
            .record_observation(obs_idx, "user_turn", short_prompt.clone());

        // --- Priority : Constitution > directive issue du trust ---
        // Le niveau utilisateur est dérivé du `TrustLevel` (User →
        // ExplicitUserInstruction) puis résolu contre la Constitution, qui
        // gagne toujours ; `can_override` verrouille qu'un ordre utilisateur
        // ne surclasse jamais la Constitution. Contenu non fiable
        // (Untrusted) : refus explicite, jamais promu en instruction.
        {
            let constitution_directive = Directive {
                level: PriorityLevel::Constitution,
                source: "Constitution".to_string(),
                rule_content: "sécurité / souveraineté / non-nuisance".to_string(),
            };
            let user_level = Self::priority_for_trust(trust_level);
            let user_directive = Directive {
                level: user_level,
                source: "User".to_string(),
                rule_content: prompt.to_string(),
            };
            let winning = self
                .priority_engine
                .resolve(&constitution_directive, &user_directive);
            let user_overrides_constitution = self
                .priority_engine
                .can_override(user_level, PriorityLevel::Constitution);
            tracing::debug!(
                "priority resolve: {:?} l'emporte (Constitution vs {:?}, surclassement={})",
                winning.level,
                user_level,
                user_overrides_constitution
            );
            if *trust_level == TrustLevel::Untrusted {
                self.self_model.enter_hesitation(
                    403,
                    "Contenu non fiable (Untrusted) : refus d'exécution, stockage seul",
                );
                self.context
                    .record_interaction("User", prompt, vec!["user_query".to_string()]);
                let refusal = "Refus explicite : contenu non fiable — stocké sans exécution, jamais promu en instruction.".to_string();
                self.context.record_interaction(
                    "Annora",
                    &refusal,
                    vec!["annora_response".to_string()],
                );
                let d = stage_start.elapsed();
                record_and_check(&mut tracker, "priority", d)?;
                self.distill_to_graph();
                record_and_check(&mut tracker, "distill", std::time::Duration::ZERO)?;
                self.record_turn_metrics(&task_id, prompt, &ConfidenceDecision::Refuse);
                let elapsed = tracker.total_elapsed().as_millis() as u64;
                return Ok(OrchestratorResult {
                    response_text: refusal,
                    routing_used: "Refusé (priorité : contenu non fiable)".to_string(),
                    latency_ms: elapsed,
                    confidence_decision: ConfidenceDecision::Refuse,
                    estimated_cost_cents: None,
                });
            }
            let d = stage_start.elapsed();
            // `priority` mappe sur max_action_ms (vérification rapide).
            record_and_check(&mut tracker, "priority", d)?;
            stage_start = Instant::now();
        }

        // --- Gate constitutionnel : HardBlock avant routing (P0) ---
        // `check_action(prompt)` + `check_action(action_type inféré)` refusent
        // les contournements documentés (prompt injection, skip audit, ...) ;
        // SoftWarning/AuditOnly ne bloquent jamais ici. `Err` ⇒ Refuse +
        // hésitation 403 (même pattern que le refus Untrusted).
        let inferred_action = Self::infer_action_type(prompt);
        let constitution_block = self
            .constitution
            .check_action(prompt)
            .and_then(|_| self.constitution.check_action(&inferred_action));
        if let Err(e) = constitution_block {
            self.self_model.enter_hesitation(
                403,
                "Refus constitutionnel : action bloquée par la Constitution",
            );
            self.context
                .record_interaction("User", prompt, vec!["user_query".to_string()]);
            let refusal = format!("Refus constitutionnel (rule) : {}", e);
            self.context.record_interaction(
                "Annora",
                &refusal,
                vec!["annora_response".to_string()],
            );
            let d = stage_start.elapsed();
            record_and_check(&mut tracker, "constitution", d)?;
            self.distill_to_graph();
            record_and_check(&mut tracker, "distill", std::time::Duration::ZERO)?;
            self.record_turn_metrics(&task_id, prompt, &ConfidenceDecision::Refuse);
            let elapsed = tracker.total_elapsed().as_millis() as u64;
            return Ok(OrchestratorResult {
                response_text: refusal,
                routing_used: "Refusé (constitution)".to_string(),
                latency_ms: elapsed,
                confidence_decision: ConfidenceDecision::Refuse,
                estimated_cost_cents: None,
            });
        }

        // 0. Automatic memory extraction from statements
        // Insensible à la casse (comparaison sur `lower`, découpe du `prompt`
        // original par offsets ASCII) + trim ponctuation (.,!?;:). Les
        // contradictions `NeedsUserResolution` sont déjà logguées (warn) +
        // stockées (`pending_resolutions`) dans `remember` — ici on accuse
        // réception explicite au lieu de `let _ =` silencieux.
        let lower = prompt.to_lowercase();
        // Recherche ASCII insensible à la casse (marqueurs ASCII ⇒ balayage
        // par octets sûr même avec UTF-8 autour). Rend l'offset d'octet.
        let find_ascii_ci = |hay: &str, needle: &str| -> Option<usize> {
            let h = hay.as_bytes();
            let n = needle.as_bytes();
            if n.is_empty() || h.len() < n.len() {
                return None;
            }
            (0..=h.len() - n.len()).find(|&i| {
                h[i..i + n.len()]
                    .iter()
                    .zip(n.iter())
                    .all(|(a, b)| a.to_ascii_lowercase() == *b)
            })
        };
        let trim_punct = |s: &str| -> String {
            s.trim()
                .trim_matches(|c: char| matches!(c, '.' | ',' | '!' | '?' | ';' | ':'))
                .trim()
                .to_string()
        };
        // Accuse réception : `remember` a déjà warn + stocké si conflit.
        let handle_auto = |res: annora_memory::InsertResult| {
            if matches!(res, annora_memory::InsertResult::NeedsUserResolution { .. }) {
                tracing::debug!(
                    "extraction auto: contradiction mise en attente (voir pending_resolutions)"
                );
            }
        };
        if !lower.contains('?') {
            if let Some(idx) = find_ascii_ci(prompt, "m'appelle") {
                let raw = prompt.get(idx + "m'appelle".len()..).unwrap_or("");
                // Garde-fou : "m'appelles"/"m'appeller…" (conjugués) ≠ marqueur.
                if !matches!(raw.chars().next(), Some(c) if c.is_alphabetic()) {
                    let name = trim_punct(raw);
                    if !name.is_empty() {
                        handle_auto(self.remember("Utilisateur", "nom", &name));
                    }
                }
            } else if let Some(idx) = find_ascii_ci(prompt, "mon nom est") {
                let name = prompt
                    .get(idx + "mon nom est".len()..)
                    .map(trim_punct)
                    .unwrap_or_default();
                if !name.is_empty() {
                    handle_auto(self.remember("Utilisateur", "nom", &name));
                }
            } else if let Some(idx) = find_ascii_ci(prompt, "s'appelle") {
                let raw_after = prompt.get(idx + "s'appelle".len()..).unwrap_or("");
                if !matches!(raw_after.chars().next(), Some(c) if c.is_alphabetic()) {
                    let subj = prompt.get(..idx).map(trim_punct).unwrap_or_default();
                    let obj = trim_punct(raw_after);
                    if !subj.is_empty() && !obj.is_empty() {
                        handle_auto(self.remember(&subj, "s'appelle", &obj));
                    }
                }
            } else {
                let prefix_len = if lower.starts_with("retiens que ") {
                    Some("retiens que ".len())
                } else if lower.starts_with("enregistre que ") {
                    Some("enregistre que ".len())
                } else {
                    None
                };
                if let Some(n) = prefix_len {
                    // Préfixe ASCII ⇒ même longueur d'octets ; `get` évite
                    // tout panic sur boundary UTF-8.
                    let fact = prompt.get(n..).map(trim_punct).unwrap_or_default();
                    if !fact.is_empty() {
                        handle_auto(self.remember("Fait", "mémorisé", &fact));
                    }
                }
            }
        }
        {
            let d = stage_start.elapsed();
            record_and_check(&mut tracker, "memory_extract", d)?;
        }

        // 1. Search relevant memory facts from KnowledgeGraph (dense semantic + keyword)
        stage_start = Instant::now();
        let mut relevant_facts = self.recall(prompt);
        for word in prompt.split_whitespace() {
            let clean = word.trim_matches(|c: char| !c.is_alphanumeric());
            if clean.len() >= 3 {
                for fact in self.recall(clean) {
                    if !relevant_facts.contains(&fact) {
                        relevant_facts.push(fact);
                    }
                }
            }
        }
        {
            let d = stage_start.elapsed();
            record_and_check(&mut tracker, "recall", d)?;
        }
        // Sense (suite) : observation du résumé de rappel en WorkingMemory.
        {
            let summary: String = if relevant_facts.is_empty() {
                "recall: aucun fait pertinent".to_string()
            } else {
                let joined = relevant_facts.join(" | ");
                format!("recall: {}", joined.chars().take(200).collect::<String>())
            };
            let idx = self.working_memory.observations.len() as u32;
            self.working_memory
                .record_observation(idx, "recall", summary);
        }

        // 2. Record interaction in continuous context
        stage_start = Instant::now();
        self.context
            .record_interaction("User", prompt, vec!["user_query".to_string()]);
        {
            let d = stage_start.elapsed();
            record_and_check(&mut tracker, "context_record", d)?;
        }

        // --- Porte dry-run : verbe irréversible → plan + carte, ZÉRO exécution ---
        // `ExecutionPlan::new()` + 1 pas irréversible, simulation locale, puis
        // réponse `CARTE REQUISE` (pastilles coût/données/réversibilité) au lieu
        // d'exécuter : `requires_human_confirmation` OU `warning.is_some()`.
        if Self::is_irreversible_prompt(prompt) {
            stage_start = Instant::now();
            // Enforcement réel : avec contexte, le plan est VÉRIFIÉ
            // (`verify_pinned` + expiry + couverture scope) avant la carte ;
            // échec = refus explicite, zéro carte, zéro exécution.
            let (plan, report, card) = match auth {
                Some((ctx, token, authority)) => {
                    match Self::build_plan_card_authorized(&task_id, prompt, ctx, token, authority)
                    {
                        Ok(built) => built,
                        Err(e) => {
                            self.self_model.enter_hesitation(
                                403,
                                format!("Plan irréversible refusé (capabilities) : {e}"),
                            );
                            return Err(e);
                        }
                    }
                }
                None => {
                    // P0 : sans token fourni, audit via token atténué court
                    // (Network+Execute, 5 min) sur authority éphémère — jamais
                    // de root longue durée. Le court ne couvre pas
                    // Wallet/Filesystem : `verify` échoue (tracé) mais la carte
                    // reste le verrou (confirmation humaine requise).
                    let (plan0, report0, card0) = Self::build_plan_card(&task_id, prompt);
                    let eph = CapabilityAuthority::new();
                    match plan0.verify_capabilities_or_short_token(None, &eph) {
                        Ok(()) => tracing::debug!(
                            "plan card: short-token audit passed (unexpected for irreversible)"
                        ),
                        Err(msg) => tracing::debug!(
                            "plan card: short-token audit (attendu pour irréversible): {msg}"
                        ),
                    }
                    (plan0, report0, card0)
                }
            };
            let total = plan.steps.len();
            let reflection = DeliberatePlanner::evaluate_reflection(&plan, 0, total);
            self.last_reflection = Some(reflection.clone());
            self.last_simulation = Some(report);
            self.self_model.set_state(CognitiveState::Simulating);
            self.self_model.enter_hesitation(
                402,
                format!(
                    "Plan '{}' : {} étape(s) irréversible(s) — confirmation humaine requise",
                    plan.plan_id, total
                ),
            );
            self.self_model
                .update_confidence((0.5 + reflection.confidence_delta).clamp(0.0, 1.0));
            self.self_model.record_action_outcome(true);
            self.context
                .record_interaction("Annora", &card, vec!["annora_response".to_string()]);
            {
                let d = stage_start.elapsed();
                record_and_check(&mut tracker, "simulation", d)?;
            }
            self.queue_turn_journal(&task_id, prompt);
            {
                let dist_start = Instant::now();
                self.distill_to_graph();
                let d = dist_start.elapsed();
                record_and_check(&mut tracker, "distill", d)?;
            }
            self.record_turn_metrics(&task_id, prompt, &ConfidenceDecision::ProposeAndWait);
            let elapsed = tracker.total_elapsed().as_millis() as u64;
            return Ok(OrchestratorResult {
                response_text: card,
                routing_used: "Plan/Dry-run (carte, zéro exécution)".to_string(),
                latency_ms: elapsed,
                confidence_decision: ConfidenceDecision::ProposeAndWait,
                estimated_cost_cents: None,
            });
        }

        // 3. Cognitive routing avec vraie entropie System 1 :
        // `system1.entropy_estimate()` (1 − confiance heuristique) au lieu du
        // 0.2 codé en dur ; tokens estimés à 4 chars/token (documenté).
        // Transition SelfModel 2/3 : Reasoning avant l'inférence.
        stage_start = Instant::now();
        self.self_model.set_state(CognitiveState::Reasoning);
        let system1_entropy = self.system1.entropy_estimate(prompt);
        let input = CognitiveInput {
            raw_prompt: prompt.to_string(),
            trust_level: trust_level.clone(),
            has_image: false,
            has_audio: false,
            estimated_token_count: prompt.len().div_ceil(4),
        };

        let decision = self.router.route(&input, system1_entropy).await;
        {
            let d = stage_start.elapsed();
            record_and_check(&mut tracker, "routing", d)?;
        }

        stage_start = Instant::now();
        // `model_confidence` réel (jamais 0.85 en dur) : S2 → score du plan,
        // S1 → 1 − entropie heuristique. `graph_corroboration` réel via
        // ConfidenceEngine (jamais 0.9 en dur), seuils par domaine.
        let mut s2_confidence_opt: Option<f64> = None;
        let (base_response, routing_str, cost_opt) = match decision {
            RoutingDecision::System1 => {
                let s1_res = self.system1.process(prompt, trust_level).await?;
                (
                    s1_res.text_response,
                    "System 1 (Local sub-100ms)".to_string(),
                    None,
                )
            }
            RoutingDecision::System2 {
                estimated_cost,
                reason: _,
            } => {
                // Enforcement réseau (P0) : provider Cloud exige une capability
                // Network/Execute valide. Chemin autorisé → `reason_with_authorization`
                // (épinglée + expiry + contexte) ; chemin historique sans token →
                // token atténué court (Network+Execute, 5 min) sur authority
                // éphémère via `reason_with_network_capability_or_short`
                // (jamais de root longue durée). Local-only → `reason` direct.
                let s2_res = match auth {
                    Some((ctx, token, authority)) => {
                        self.system2
                            .reason_with_authorization(
                                prompt,
                                &relevant_facts,
                                trust_level,
                                ctx,
                                authority,
                                Some(token),
                                Self::EXPECTED_AUDIENCE,
                            )
                            .await?
                    }
                    None => {
                        if self.system2.provider().is_cloud() {
                            let eph = CapabilityAuthority::new();
                            self.system2
                                .reason_with_network_capability_or_short(
                                    prompt,
                                    &relevant_facts,
                                    trust_level,
                                    &eph,
                                    None,
                                )
                                .await?
                        } else {
                            self.system2
                                .reason(prompt, &relevant_facts, trust_level)
                                .await?
                        }
                    }
                };
                s2_confidence_opt = Some(s2_res.confidence_score);
                // Plan-Execute-Reflect (§28b) : après S2 on construit un plan
                // exécutable honnête (1 étape lecture/inférence par segment S2,
                // réversibles) et on évalue la réflexion immédiatement.
                // Heuristique : tous les segments S2 sont considérés réussis
                // (pas d'exécution réelle ici) — la réflexion sert l'audit.
                let reflect_start = Instant::now();
                let mut plan = ExecutionPlan::new(
                    format!("s2-{}", prompt.len() % 10000),
                    prompt.chars().take(120).collect::<String>(),
                );
                for step in &s2_res.proposed_steps {
                    plan.add_step(
                        step.clone(),
                        CapabilityScope::ModelInference,
                        CapabilityAction::Read,
                        false,
                    );
                }
                let total = plan.steps.len();
                let reflection = DeliberatePlanner::evaluate_reflection(&plan, total, total);
                tracing::debug!(
                    plan_id = %reflection.plan_id,
                    summary = %reflection.summary,
                    "reflection après S2"
                );
                self.last_reflection = Some(reflection.clone());
                self.self_model.update_confidence(
                    (s2_res.confidence_score + reflection.confidence_delta).clamp(0.0, 1.0),
                );
                let d_reflect = reflect_start.elapsed();
                // Enregistre la réflexion dans son propre stage budget.
                tracker.record_stage("reflection", d_reflect);
                // Per-stage best-effort : on ne fait pas aborter le tour si la
                // réflexion seule dépasse (micro-stage), mais on trace.
                if let Err(e) = tracker.check_stage("reflection", d_reflect) {
                    tracing::warn!("reflection hors budget per-stage: {}", e);
                }
                (
                    s2_res.synthesized_answer,
                    "System 2 (Distant deep reasoning)".to_string(),
                    Some(estimated_cost.amount_cents),
                )
            }
        };

        let response = if !relevant_facts.is_empty()
            && (prompt.contains('?')
                || lower.contains("comment")
                || lower.contains("quel")
                || lower.contains("qui"))
        {
            format!(
                "D'après votre graphe de mémoire souverain : {}\n({})",
                relevant_facts.join(" | "),
                base_response
            )
        } else {
            base_response
        };

        // Inference stage (S1/S2 ci-dessus).
        {
            let d = stage_start.elapsed();
            record_and_check(&mut tracker, "inference", d)?;
        }

        // --- Dry-run avant action irréversible (§24d) ---
        // `action_type` inféré par heuristique ; si contient payment/file/delete
        // on simule + log tracing AVANT toute exécution (ici : avant réponse).
        {
            let sim_start = Instant::now();
            let action_type = Self::infer_action_type(prompt);
            if let Some(report) = self.dry_run_for_action(&action_type, prompt) {
                self.last_simulation = Some(report);
            }
            let d = sim_start.elapsed();
            tracker.record_stage("simulation", d);
            if let Err(e) = tracker.check_stage("simulation", d) {
                tracing::warn!("simulation hors budget per-stage: {}", e);
            }
        }

        // --- Confidence par domaine (jamais 0.85/0.9 en dur) ---
        stage_start = Instant::now();
        let domain = ConfidenceEngine::infer_domain(prompt);
        let graph_corr = ConfidenceEngine::corroboration_from_facts(&relevant_facts);
        let model_conf = s2_confidence_opt.unwrap_or((1.0 - system1_entropy).clamp(0.0, 1.0));
        let action_type = Self::infer_action_type(prompt);
        let proposed = ProposedAction {
            action_name: action_type,
            domain,
            model_confidence: model_conf,
            parameters_summary: prompt.chars().take(200).collect(),
        };
        let (conf_decision, conf_level) = self.confidence_engine.evaluate(&proposed, graph_corr);
        self.self_model.update_confidence(conf_level.combined);

        // --- Contradiction interne quand zone limite ---
        // `InternalContradiction` = zone limite du ConfidenceEngine : on
        // critique la réponse contre les faits mémoire (jamais silencieux).
        if conf_decision == ConfidenceDecision::InternalContradiction {
            let contra_start = Instant::now();
            match self
                .contradiction_engine
                .critique(&response, &relevant_facts)
            {
                Ok(analysis) => {
                    tracing::info!(
                        has_contradiction = analysis.has_contradiction,
                        penalty = analysis.confidence_penalty,
                        "critique interne en zone limite"
                    );
                    self.last_contradiction = Some(analysis);
                }
                Err(e) => {
                    tracing::warn!("critique interne impossible: {}", e);
                    self.last_contradiction = None;
                }
            }
            let d = contra_start.elapsed();
            tracker.record_stage("contradiction", d);
            if let Err(e) = tracker.check_stage("contradiction", d) {
                tracing::warn!("contradiction hors budget per-stage: {}", e);
            }
        }

        // --- Gate confiance RÉEL : match exhaustif, jamais d'exécution silencieuse ---
        // AutoExecute → on continue ; ProposeAndWait/Refuse → attente/refus
        // explicite + hésitation tracée ; InternalContradiction → critique déjà
        // jouée ci-dessus : contradiction avérée ⇒ pause + `confidence_penalty`
        // appliquée, autocritique propre ⇒ on procède en AutoExecute.
        let (final_response, final_decision) = match conf_decision {
            ConfidenceDecision::AutoExecute => (response, ConfidenceDecision::AutoExecute),
            ConfidenceDecision::InternalContradiction => {
                match self.last_contradiction.clone() {
                    Some(analysis) if analysis.has_contradiction => {
                        let adjusted = (conf_level.combined - analysis.confidence_penalty).max(0.0);
                        self.self_model.enter_hesitation(
                            409,
                            format!(
                                "Contradiction interne : {} — trancher avant exécution",
                                analysis.counter_argument
                            ),
                        );
                        self.self_model.update_confidence(adjusted);
                        let wait = format!(
                            "Je marque une pause : contradiction interne détectée.\n  • {}\n  • Pénalité de confiance appliquée : {:.2} (confiance ajustée : {:.2}).\nAucune exécution — dites-moi comment trancher.",
                            analysis.counter_argument, analysis.confidence_penalty, adjusted
                        );
                        (wait, ConfidenceDecision::InternalContradiction)
                    }
                    _ => {
                        // Autocritique propre (ou critique en échec, infaillible
                        // en pratique) : on procède, décision redressée.
                        self.self_model.update_confidence(conf_level.combined);
                        (response, ConfidenceDecision::AutoExecute)
                    }
                }
            }
            ConfidenceDecision::ProposeAndWait => {
                self.self_model.enter_hesitation(
                    401,
                    "Confiance sous le seuil d'auto-exécution : proposition en attente",
                );
                self.self_model.update_confidence(conf_level.combined);
                let wait = format!(
                    "Je vous propose la réponse suivante, mais ma confiance ({:.2}) est sous le seuil d'exécution automatique :\n{}\nConfirmez pour que j'aille plus loin — rien n'est exécuté.",
                    conf_level.combined, response
                );
                (wait, ConfidenceDecision::ProposeAndWait)
            }
            ConfidenceDecision::Refuse => {
                self.self_model.enter_hesitation(
                    403,
                    "Confiance sous le seuil de proposition : refus explicite",
                );
                self.self_model.update_confidence(conf_level.combined);
                let refusal = format!(
                    "Refus explicite : ma confiance ({:.2}) est sous le seuil de proposition pour ce domaine — je n'exécute rien.",
                    conf_level.combined
                );
                (refusal, ConfidenceDecision::Refuse)
            }
        };

        // Transition SelfModel 3/3 : Executing avant l'enregistrement final.
        // `set_state` efface le rationnel d'hésitation (cf. `SelfModel`) :
        // les chemins d'attente/refus ré-affirment l'hésitation juste après.
        self.self_model.set_state(CognitiveState::Executing);
        if final_decision != ConfidenceDecision::AutoExecute {
            let (code, why) = match &final_decision {
                ConfidenceDecision::InternalContradiction => (
                    409,
                    self.last_contradiction
                        .as_ref()
                        .map(|a| a.counter_argument.clone())
                        .unwrap_or_else(|| "contradiction interne".to_string()),
                ),
                ConfidenceDecision::ProposeAndWait => {
                    (401, "proposition en attente de confirmation".to_string())
                }
                _ => (403, "refus explicite".to_string()),
            };
            self.self_model.enter_hesitation(code, why);
        }
        // Observation de sortie en WorkingMemory (traçabilité éphémère).
        let short_response: String = final_response.chars().take(200).collect();
        self.working_memory.record_observation(
            self.working_memory.observations.len() as u32,
            routing_str.clone(),
            short_response,
        );

        // 4. Record system response in context
        self.context.record_interaction(
            "Annora",
            &final_response,
            vec!["annora_response".to_string()],
        );
        {
            let d = stage_start.elapsed();
            // `response_record` mappe sur max_action_ms.
            record_and_check(&mut tracker, "response_record", d)?;
        }

        // Reflect après Act : résultat + confiance roulante (les plans S2 et
        // carte irréversible ont déjà leur `evaluate_reflection` dédiée).
        self.self_model.record_action_outcome(true);

        // Distillation systématique AVANT Ok : file → `remember()` (graphe).
        self.queue_turn_journal(&task_id, prompt);
        {
            let dist_start = Instant::now();
            self.distill_to_graph();
            let d = dist_start.elapsed();
            record_and_check(&mut tracker, "distill", d)?;
        }

        // P3 : continuité + généralité + initiative drainées après chaque tour.
        self.record_turn_metrics(&task_id, prompt, &final_decision);

        let elapsed = tracker.total_elapsed().as_millis() as u64;

        Ok(OrchestratorResult {
            response_text: final_response,
            routing_used: routing_str,
            latency_ms: elapsed,
            confidence_decision: final_decision,
            estimated_cost_cents: cost_opt,
        })
    }
}

#[cfg(test)]
mod capability_enforcement_tests {
    use super::*;
    use annora_core::{CapabilityAuthority, Timestamp};

    fn future_ts() -> Timestamp {
        Timestamp(chrono::Utc::now() + chrono::Duration::hours(1))
    }

    fn past_ts() -> Timestamp {
        Timestamp(chrono::Utc::now() - chrono::Duration::hours(1))
    }

    fn full_ctx(resource: &str, expiry: Timestamp) -> AuthorizationContext {
        AuthorizationContext::new(
            "user:test",
            CognitiveOrchestrator::EXPECTED_AUDIENCE,
            expiry,
            CapabilityScope::FullSystem,
            CapabilityAction::Admin,
            resource,
            None,
            "nonce-orch-1234abcd",
            vec![],
        )
        .expect("contexte valide")
    }

    #[test]
    fn authorized_card_succeeds_with_valid_capabilities() {
        let authority = CapabilityAuthority::new();
        // Prompt irréversible Filesystem/Delete → token couvrant.
        let token = authority
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Delete],
                future_ts(),
            )
            .unwrap();
        let ctx = full_ctx("notes/todo.md", future_ts());
        let (plan, _report, card) = CognitiveOrchestrator::build_plan_card_authorized(
            "t1",
            "supprime /vfs/notes.txt",
            &ctx,
            &token,
            &authority,
        )
        .expect("capabilities suffisantes");
        assert!(plan.requires_human_confirmation);
        assert!(card.contains("CARTE REQUISE"));
    }

    #[test]
    fn authorized_card_refuses_expired_token() {
        let authority = CapabilityAuthority::new();
        let token = authority
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Delete],
                past_ts(),
            )
            .unwrap();
        let ctx = full_ctx("notes/todo.md", future_ts());
        let err = CognitiveOrchestrator::build_plan_card_authorized(
            "t2",
            "supprime /vfs/notes.txt",
            &ctx,
            &token,
            &authority,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("expiré") || err.to_string().contains("Expired"),
            "{err}"
        );
    }

    #[test]
    fn authorized_card_refuses_out_of_scope_token() {
        let authority = CapabilityAuthority::new();
        // Token Network/Execute ne couvre pas le pas Filesystem/Delete.
        let token = authority
            .create_root_token(
                vec![CapabilityScope::Network],
                vec![CapabilityAction::Execute],
                future_ts(),
            )
            .unwrap();
        let ctx = full_ctx("notes/todo.md", future_ts());
        let err = CognitiveOrchestrator::build_plan_card_authorized(
            "t3",
            "supprime /vfs/notes.txt",
            &ctx,
            &token,
            &authority,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("scope insuffisant")
                || err.to_string().contains("capability check"),
            "{err}"
        );
    }

    #[test]
    fn verify_plan_refuses_audience_mismatch_and_traversal() {
        let authority = CapabilityAuthority::new();
        let token = authority
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Delete],
                future_ts(),
            )
            .unwrap();
        let (plan, _, _) = CognitiveOrchestrator::build_plan_card("t4", "supprime /vfs/notes.txt");
        let ctx = full_ctx("notes/todo.md", future_ts());
        // Audience attendue = annora-kernel ; forcer un mismatch via l'API plan.
        let err = plan
            .verify_capabilities_with_context(&token, &authority, &ctx, "annora-daemon")
            .unwrap_err();
        assert!(err.contains("audience") || err.contains("rejeté"), "{err}");
        // Traversal : construction du contexte refusée (jamais un plan).
        assert!(
            AuthorizationContext::new(
                "user:test",
                CognitiveOrchestrator::EXPECTED_AUDIENCE,
                future_ts(),
                CapabilityScope::Filesystem,
                CapabilityAction::Read,
                "../evil",
                None,
                "nonce-orch-1234abcd",
                vec![],
            )
            .is_err()
        );
        // Hors-VFS absolu refusé.
        assert!(
            AuthorizationContext::new(
                "user:test",
                CognitiveOrchestrator::EXPECTED_AUDIENCE,
                future_ts(),
                CapabilityScope::Filesystem,
                CapabilityAction::Read,
                "/etc/passwd",
                None,
                "nonce-orch-1234abcd",
                vec![],
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn authorized_turn_on_irreversible_enforces_capabilities() {
        use crate::fabric::{CognitiveFabric, CognitiveManifest};
        let fabric = CognitiveFabric::new(CognitiveManifest::default());
        let constitution = Constitution::default_v1();
        let mut orch =
            CognitiveOrchestrator::new(fabric, UserId(uuid::Uuid::now_v7()), constitution);
        let authority = CapabilityAuthority::new();
        // Token insuffisant (Network) pour une suppression Filesystem.
        let bad_token = authority
            .create_root_token(
                vec![CapabilityScope::Network],
                vec![CapabilityAction::Execute],
                future_ts(),
            )
            .unwrap();
        let ctx = full_ctx("notes/todo.md", future_ts());
        let err = orch
            .process_user_turn_with_authorization(
                "supprime /vfs/notes.txt",
                &TrustLevel::User,
                &ctx,
                &bad_token,
                &authority,
            )
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("scope insuffisant") || err.to_string().contains("capability"),
            "{err}"
        );
        // Token couvrant → carte, zéro exécution, confirmation requise.
        // Contexte ajusté au pas du plan (Filesystem/Delete) : le gate
        // d'entrée exige que le token couvre le scope du contexte lui-même.
        let good_token = authority
            .create_root_token(
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Delete],
                future_ts(),
            )
            .unwrap();
        let good_ctx = AuthorizationContext::new(
            "user:test",
            CognitiveOrchestrator::EXPECTED_AUDIENCE,
            future_ts(),
            CapabilityScope::Filesystem,
            CapabilityAction::Delete,
            "/vfs/notes.txt",
            None,
            "nonce-orch-1234abcd",
            vec![],
        )
        .expect("contexte valide");
        let res = orch
            .process_user_turn_with_authorization(
                "supprime /vfs/notes.txt",
                &TrustLevel::User,
                &good_ctx,
                &good_token,
                &authority,
            )
            .await
            .expect("capabilities suffisantes");
        assert!(res.response_text.contains("CARTE REQUISE"));
        assert_eq!(res.confidence_decision, ConfidenceDecision::ProposeAndWait);
    }
}
