//! Boucle délibérée Sense→Plan→Act→Reflect (mission LOOP).
//!
//! - prompt bénin : pas de carte, réponse non vide ;
//! - prompt irréversible (FR/EN) : carte `CARTE REQUISE`, zéro exécution ;
//! - contradiction FR (`supprime` vs `conserve`) : pénalité appliquée ;
//! - budget abort conservé (budget nul ⇒ erreur explicite) ;
//! - enveloppe WorkingMemory : journal distillé vers le graphe ;
//! - curiosité `using_graph_orphans` + `plan_preview` (daemon `:plan`).

use hakille_constitution::rules::Constitution;
use hakille_core::{ConfidenceDecision, TrustLevel, UserId};
use hakille_kernel::{
    GraphCuriosityScanner,
    budget::PipelineBudgetConfig,
    fabric::{CognitiveFabric, CognitiveManifest},
    orchestrator::CognitiveOrchestrator,
};
use uuid::Uuid;

fn test_orchestrator() -> CognitiveOrchestrator {
    let fabric = CognitiveFabric::new(CognitiveManifest::default());
    let user_id = UserId(Uuid::now_v7());
    CognitiveOrchestrator::new(fabric, user_id, Constitution::default_v1())
}

/// Orchestrateur prêt pour un tour réel avec décodage Qwen CPU (minutes en
/// debug) : budget pipeline + timeouts LLM alignés juste après création. Le
/// défaut (100 ms d'inférence, 600 s de timeout LLM) aborterait par construction.
fn test_orchestrator_real_llm() -> CognitiveOrchestrator {
    let mut orch = test_orchestrator();
    orch.set_budget_config(PipelineBudgetConfig::aligned_with_llm_timeout(3_600_000));
    orch.system1.set_llm_timeout(3_600_000);
    orch.system2.set_llm_timeout(3_600_000);
    orch
}

#[tokio::test]
#[ignore = "slow: tour complet avec décodage Qwen CPU réel (minutes en debug) ; gate lente `-- --ignored`"]
async fn test_deliberate_loop_benign_no_card_non_empty() {
    let mut orch = test_orchestrator_real_llm();
    let res = orch
        .process_user_turn("Bonjour Hakille, comment vas-tu ?", &TrustLevel::User)
        .await
        .expect("benign turn failed");
    assert!(!res.response_text.is_empty(), "réponse non vide");
    assert!(
        !res.response_text.contains("CARTE REQUISE"),
        "pas de carte sur prompt bénin, obtenu : {}",
        res.response_text
    );
    assert_eq!(orch.context.conversation_stream.len(), 2);
}

#[tokio::test]
async fn test_deliberate_loop_irreversible_payment_card_no_execution() {
    let mut orch = test_orchestrator();
    let res = orch
        .process_user_turn("Transfère 250 EUR à Alice pour le loyer", &TrustLevel::User)
        .await
        .expect("payment turn failed");
    assert!(
        res.response_text.contains("CARTE REQUISE"),
        "carte attendue, obtenu : {}",
        res.response_text
    );
    assert!(
        res.response_text.contains("250"),
        "montant simulé dans la carte, obtenu : {}",
        res.response_text
    );
    assert!(
        res.response_text.contains("réversibilité"),
        "pastille réversibilité attendue, obtenu : {}",
        res.response_text
    );
    assert_eq!(res.confidence_decision, ConfidenceDecision::ProposeAndWait);
    // Zéro exécution : plan 0/total réussi + simulation archivée pour audit.
    let reflection = orch.last_reflection.as_ref().expect("réflexion archivée");
    assert!(!reflection.all_steps_succeeded);
    assert!(orch.last_simulation.is_some());
    assert_eq!(orch.context.conversation_stream.len(), 2);
}

#[tokio::test]
async fn test_deliberate_loop_irreversible_delete_card_en() {
    let mut orch = test_orchestrator();
    let res = orch
        .process_user_turn("Delete the file /tmp/brouillon.txt now", &TrustLevel::User)
        .await
        .expect("delete turn failed");
    assert!(
        res.response_text.contains("CARTE REQUISE"),
        "carte EN attendue, obtenu : {}",
        res.response_text
    );
    assert!(
        res.response_text.contains("/tmp/brouillon.txt"),
        "cible fichier dans la carte, obtenu : {}",
        res.response_text
    );
    assert_eq!(res.confidence_decision, ConfidenceDecision::ProposeAndWait);
}

// La critique couvre désormais le prompt ET la réponse : « Conserve » doit
// être détecté face au fait mémorisé « supprime », même si Qwen répond un
// refus générique. Test lent (décodage Qwen CPU réel).
#[tokio::test]
#[ignore = "slow: tour complet avec décodage Qwen CPU réel (minutes en debug) ; gate lente `-- --ignored`"]
async fn test_deliberate_loop_fr_contradiction_penalty_applied() {
    let mut orch = test_orchestrator_real_llm();
    match orch.remember(
        "Document",
        "consigne",
        "supprime les brouillons chaque soir",
    ) {
        hakille_memory::InsertResult::Inserted(_) => {}
        other => panic!("insertion initiale attendue, obtenue : {:?}", other),
    }
    // « Conserve » (conserve-side) vs fait « supprime » (supprime-side),
    // sans verbe déclencheur de carte : la gate confiance doit pauser.
    let res = orch
        .process_user_turn(
            "Conserve tous les brouillons sans exception",
            &TrustLevel::User,
        )
        .await
        .expect("contradiction turn failed");
    assert_eq!(
        res.confidence_decision,
        ConfidenceDecision::InternalContradiction
    );
    assert!(
        !res.response_text.contains("CARTE REQUISE"),
        "pause contradiction, pas carte : {}",
        res.response_text
    );
    assert!(
        res.response_text.to_lowercase().contains("contradiction"),
        "pause explicite attendue, obtenu : {}",
        res.response_text
    );
    assert!(
        res.response_text.contains("0.35"),
        "pénalité appliquée et affichée, obtenu : {}",
        res.response_text
    );
    let analysis = orch.last_contradiction.as_ref().expect("analyse archivée");
    assert!(analysis.has_contradiction);
    assert!((analysis.confidence_penalty - 0.35).abs() < 1e-9);
    assert!(orch.self_model.query_hesitation_reason().is_some());
}

#[tokio::test]
async fn test_deliberate_loop_budget_abort_preserved() {
    use hakille_kernel::budget::PipelineBudgetConfig;
    let mut orch = test_orchestrator();
    orch.set_budget_config(PipelineBudgetConfig {
        max_perception_ms: 0,
        max_routing_ms: 0,
        max_inference_ms: 0,
        max_action_ms: 0,
        max_total_ms: 0,
    });
    let res = orch.process_user_turn("Bonjour", &TrustLevel::User).await;
    assert!(res.is_err(), "budget nul ⇒ abort, pas de continuation");
    assert!(
        format!("{:?}", res.unwrap_err()).contains("budget"),
        "erreur de budget explicite"
    );
}

#[tokio::test]
#[ignore = "slow: tours complets avec décodage Qwen CPU réel (minutes en debug) ; gate lente `-- --ignored`"]
async fn test_deliberate_loop_working_memory_distilled_to_graph() {
    let mut orch = test_orchestrator_real_llm();
    assert!(orch.memory_graph.is_empty());
    let _ = orch
        .process_user_turn("Bonjour", &TrustLevel::User)
        .await
        .expect("journal turn failed");
    // L'enveloppe Sense→Reflect a persisté le journal du tour (sujet unique).
    assert_eq!(
        orch.memory_graph.len(),
        1,
        "1 fait journal distillé vers le graphe"
    );
    assert!(
        orch.recall("Bonjour").contains(&"Bonjour".to_string()),
        "journal retrouvable"
    );
}

#[test]
fn test_deliberate_loop_curiosity_orphan_mode() {
    let mut orch = test_orchestrator();
    for (s, p, o) in [("ProjetX", "statut", "en cours"), ("Alice", "aime", "Rust")] {
        match orch.remember(s, p, o) {
            hakille_memory::InsertResult::Inserted(_) => {}
            other => panic!("insertion attendue, obtenue : {:?}", other),
        }
    }
    // Sans sujet récent : les 2 sujets sont orphelins.
    let orphans = GraphCuriosityScanner::using_graph_orphans(&orch.memory_graph, &[]);
    assert_eq!(orphans.len(), 2, "2 sujets orphelins : {:?}", orphans);
    // « ProjetX » déjà abordé : seul « Alice » relancée.
    let filtered =
        GraphCuriosityScanner::using_graph_orphans(&orch.memory_graph, &["projetx".to_string()]);
    assert_eq!(filtered.len(), 1);
    assert!(
        filtered[0].question_prompt.contains("Alice"),
        "relance ciblée : {}",
        filtered[0].question_prompt
    );
    // Tout couvert : silence (staging, pas d'interruption).
    let silent = GraphCuriosityScanner::using_graph_orphans(
        &orch.memory_graph,
        &["projetx".to_string(), "alice".to_string()],
    );
    assert!(silent.is_empty());
}

#[test]
fn test_deliberate_loop_plan_preview() {
    let orch = test_orchestrator();
    let card = orch.plan_preview("Supprime le dossier /tmp/vieux");
    assert!(card.contains("CARTE REQUISE"), "preview carte : {}", card);
    let quiet = orch.plan_preview("Quel temps fait-il ?");
    assert!(
        quiet.contains("AUCUNE CARTE REQUISE"),
        "preview bénin : {}",
        quiet
    );
}

#[test]
fn test_deliberate_loop_irreversible_detector() {
    // Verbes spec FR/EN.
    for p in [
        "Paye la facture",
        "Supprime ce fichier",
        "Envoie le rapport",
        "Transférer 25 EUR",
        "Please pay the invoice",
        "Delete everything",
        "Send the document",
        "Transfer funds now",
        "Unlock the vault",
    ] {
        assert!(
            CognitiveOrchestrator::is_irreversible_prompt(p),
            "déclencheur attendu : {}",
            p
        );
    }
    // Bénins : nation « pays », « nom »/« non », évocations.
    for p in [
        "Quel pays visites-tu ?",
        "Quel est mon nom ?",
        "Explique le paiement SEPA",
        "Bonjour Hakille",
    ] {
        assert!(
            !CognitiveOrchestrator::is_irreversible_prompt(p),
            "pas de carte attendue : {}",
            p
        );
    }
}
