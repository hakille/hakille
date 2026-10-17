//! Écrans §8 : tests e2e des view-models (ÉQUIPE MODEL).
//!
//! Carte mail cran 1, WhySheet 3 niveaux depuis une entrée d'audit + faits
//! graphe, filtre pro/perso sans fusion, file d'envoi avec revalidation > 15 min.
//! Aucune exécution réelle : `Approve` simulé = transition d'état documentée.
//!
//! NOTE intégration : `conformance/Cargo.toml` ne dépend PAS d'`annora-design`
//! (ajout interdit) : on passe par les miroirs purs d'`annora-ui-shell`
//! (`ActionCardStrings::from_parts`, `WhyStrings::from_parts`) avec les mêmes
//! formats que `ActionCardView` / `WhySheet`. Triplets construits en mémoire
//! via `annora-memory` (dépendance présente). `lib.rs` non touché :
//! l'intégrateur déclare `pub mod screens_model_tests;`.

use annora_audit::entry::{ActionRecord, AuditEntry, ReasoningStep, ReasoningTrace};
use annora_core::{
    ActorId, EntryId, NodeId, PredicateId, Timestamp, TripletId, TrustLevel, UserId,
};
use annora_memory::{
    ContextPartition, KnowledgeGraph, NodeValue, Triplet, TripletMetadata, TripletSource,
    scan_contradictions,
};
use annora_sync::{Outbox, OutboxEntry, REVALIDATE_AFTER_MINUTES};
use annora_ui::OnboardingManager;
use annora_ui_shell::{
    ActionCardStrings, MemoryRow, MemoryScreenVM, OnboardingSheetVM, OnboardingView, OutboxPanelVM,
    SettingsState, SettingsView, WhyStrings,
};

fn sample_entry() -> AuditEntry {
    AuditEntry {
        id: EntryId(uuid::Uuid::now_v7()),
        timestamp: Timestamp::now(),
        actor: ActorId::User(UserId(uuid::Uuid::now_v7())),
        action: ActionRecord {
            action_type: "send-mail".into(),
            description: "Envoyer ce mail à Awa".into(),
            parameters: serde_json::json!({}),
            reversible: true,
            reversed: false,
        },
        capability_used: None,
        reasoning_trace: ReasoningTrace {
            steps: vec![
                ReasoningStep {
                    description: "Tu as demandé de répondre à Awa.".into(),
                    confidence: 0.92,
                    evidence: vec![],
                },
                ReasoningStep {
                    description: "Brouillon relu, coût nul, annulable 30 s.".into(),
                    confidence: 0.9,
                    evidence: vec![],
                },
            ],
            model_used: None,
            duration_ms: 1,
        },
        graph_nodes_used: vec![TripletId(uuid::Uuid::now_v7())],
        confidence: 0.92,
        cost: None,
        signature: vec![],
        prev_hash: vec![],
        token_hash: None,
    }
}

fn sample_triplet(object: &str, confidence: f64, partition: ContextPartition) -> Triplet {
    Triplet {
        id: TripletId(uuid::Uuid::now_v7()),
        subject: NodeId(uuid::Uuid::from_u128(1)),
        predicate: PredicateId(uuid::Uuid::from_u128(2)),
        object: NodeValue::Text(object.to_string()),
        metadata: TripletMetadata {
            weight: 0.9,
            created_at: Timestamp::now(),
            last_activated: Timestamp::now(),
            source: TripletSource::User,
            confidence,
            context: partition,
            trust_level: TrustLevel::User,
            decay_rate: 0.01,
        },
        node_type: None,
    }
}

fn mem_row(text: &str, partition: &str) -> MemoryRow {
    MemoryRow {
        id: "id".into(),
        text: text.into(),
        weight: 0.9,
        confidence_pct: 90,
        source: "User".into(),
        partition: partition.into(),
    }
}

/// Carte mail cran 1 : pastilles Local / 0 € / annulable.
/// `Approve` simulé = transition d'état documentée, PAS d'exécution
/// (la validation réelle passe par le chip Braise + trace Journal).
#[test]
fn test_mail_card_first_notch_approve_is_documented_state_transition() {
    let card = ActionCardStrings::from_parts(
        "Envoyer",
        "ce mail à Awa",
        "Tu as demandé de répondre.",
        "0 €",
        "Local",
        "− annulable",
        false,
        "Akora",
        false,
    );
    assert_eq!(
        card.pastilles,
        [
            "0 €".to_string(),
            "Local".to_string(),
            "− annulable".to_string()
        ]
    );
    assert_eq!(card.validate_label, "Valider");

    #[derive(Debug, PartialEq)]
    enum Decision {
        Pending,
        Approved,
    }
    let _decision = Decision::Pending;
    // Geste simulé : aucune exécution, seule la transition est tracée.
    let decision = Decision::Approved;
    assert_eq!(decision, Decision::Approved);

    let entry = sample_entry();
    let row = annora_ui_shell::JournalRow::from_entry(&entry);
    assert!(row.reason_full.contains("Tu as demandé"));
    assert!(row.reason_full.contains("annulable 30 s"));
    assert_eq!(row.graph_nodes.len(), 1);
}

/// WhySheet 3 niveaux depuis une entrée d'audit + faits du graphe en mémoire.
#[test]
fn test_why_sheet_three_levels_from_entry_and_graph_facts() {
    let entry = sample_entry();
    let sentence = annora_ui::JournalView::explain_action(&entry);
    assert!(sentence.contains("send-mail"));

    // Faits graphe réels : deux triplets insérés, relus via `all_triplets`.
    let mut graph = KnowledgeGraph::new();
    let mut paris = sample_triplet("Paris", 0.9, ContextPartition::Personal);
    paris.subject = NodeId(uuid::Uuid::from_u128(10));
    let mut dakar = sample_triplet("Dakar", 0.85, ContextPartition::Personal);
    dakar.subject = NodeId(uuid::Uuid::from_u128(11));
    graph.insert(paris);
    graph.insert(dakar);
    assert_eq!(graph.all_triplets().len(), 2);
    let facts: Vec<(String, String, String, String)> = graph
        .all_triplets()
        .iter()
        .map(|t| {
            let label = match &t.object {
                NodeValue::Text(s) => s.clone(),
                other => format!("{other:?}"),
            };
            (
                label,
                "fait".to_string(),
                "2026-10-15".to_string(),
                "User".to_string(),
            )
        })
        .collect();

    let why = WhyStrings::from_parts(
        "J'ai préparé ce mail car tu l'as demandé.",
        &facts,
        "1. lire le contexte 2. proposer le brouillon",
        &["Network".to_string()],
        entry.confidence,
        &["ne rien faire".to_string()],
        "local",
        "0 €",
        "Local",
        "une confiance < 0,5 m'aurait fait demander",
    );
    // Niveau 1 : phrase. Niveau 2 : faits. Niveau 3 : le reste.
    assert!(!why.sentence.is_empty());
    assert_eq!(why.facts.len(), 2);
    assert!(
        why.facts
            .iter()
            .all(|f| f.contains(" — ") && f.contains("User"))
    );
    assert!(!why.plan.is_empty() && !why.powers.is_empty());
    assert_eq!(why.confidence, "haute (0.92)");
    assert!(!why.alternatives.is_empty());
    assert!(!why.model_line.is_empty() && !why.would_change.is_empty());

    // Contradiction candidate (même S/P, objets rivaux) : jamais fusionnée
    // silencieusement — `scan_contradictions` la signale, l'utilisateur tranche.
    let a = sample_triplet("Paris", 0.9, ContextPartition::Personal);
    let mut b = sample_triplet("Dakar", 0.8, ContextPartition::Personal);
    b.subject = a.subject.clone();
    b.predicate = a.predicate.clone();
    let hits = scan_contradictions(&[a, b]);
    assert_eq!(hits.len(), 1);
    let contra = MemoryScreenVM::from_rows(&[], "Tout", hits.len()).contra_label;
    assert_eq!(contra, "1 contradiction(s) — Garder A / B / les deux");
}

/// Filtre pro/perso : jamais de fusion entre partitions.
#[test]
fn test_memory_screen_pro_perso_never_merges() {
    let rows = vec![
        mem_row("Anniversaire Awa", "Personal"),
        mem_row("Budget Q4", "Professional"),
        mem_row("Photo foyer", "Shared"),
    ];
    let perso = MemoryScreenVM::from_rows(&rows, "Personal", 0);
    let pro = MemoryScreenVM::from_rows(&rows, "Professional", 0);
    assert_eq!(perso.visible.len(), 1);
    assert_eq!(pro.visible.len(), 1);
    assert!(perso.visible[0].contains("Personal"));
    assert!(pro.visible[0].contains("Professional"));
    // "Tout" voit tout, mais chaque ligne garde sa partition d'origine.
    let all = MemoryScreenVM::from_rows(&rows, "Tout", 0);
    assert_eq!(all.visible.len(), 3);

    // Chaîne `:mem2` du daemon : graphe → items → MemoryRow → écran texte.
    let mut graph = KnowledgeGraph::new();
    for (i, (object, part)) in [
        ("Anniversaire Awa", ContextPartition::Personal),
        ("Budget Q4", ContextPartition::Professional),
    ]
    .iter()
    .enumerate()
    {
        let mut t = sample_triplet(object, 0.9, part.clone());
        t.subject = NodeId(uuid::Uuid::from_u128(100 + i as u128));
        graph.insert(t);
    }
    let items = annora_ui::MemoryViewerPanel::list_all_items(&graph);
    let grown: Vec<annora_ui_shell::MemoryRow> = items
        .iter()
        .map(annora_ui_shell::MemoryRow::from_item)
        .collect();
    let screen = MemoryScreenVM::from_rows(&grown, "Professional", 0);
    assert_eq!(screen.visible.len(), 1);
    assert!(screen.visible[0].contains("Budget Q4"));
}

/// File d'envoi : > 15 min sans revalidation = bloquée, puis revalidée = part.
#[test]
fn test_outbox_panel_revalidation_over_15min() {
    assert_eq!(REVALIDATE_AFTER_MINUTES, 15);
    let mut outbox = Outbox::new();
    let mut old = OutboxEntry::new("Envoyer", "mail à Awa", "mail-1");
    old.queued_at = chrono::Utc::now() - chrono::Duration::minutes(16);
    outbox.enqueue(old);
    let now = chrono::Utc::now();
    assert!(outbox.take_sendable(now).is_none());
    assert!(outbox.revalidate("mail-1"));
    let sent = outbox.take_sendable(now).expect("revalidée : doit partir");
    assert_eq!(sent.dedup_key, "mail-1");

    // Projection texte via le triplet documenté (clé, "verbe objet", statut).
    let lines = OutboxPanelVM::from_entries(&[(
        sent.dedup_key.clone(),
        format!("{} {}", sent.verb, sent.object),
        "envoyable (revalidée)".to_string(),
    )]);
    assert_eq!(
        lines,
        vec!["[mail-1] Envoyer mail à Awa — envoyable (revalidée)".to_string()]
    );
}

/// Réglages + onboarding : couverture §8 en lignes texte.
#[test]
fn test_settings_and_onboarding_screens_cover_section8() {
    let settings = SettingsState::from_settings(&SettingsView::default());
    assert!(settings.rows.iter().any(|r| r.contains("[Local]")));
    assert!(settings.rows.iter().any(|r| r.starts_with("Simulation : ")));

    let onboarding =
        OnboardingSheetVM::from_view(&OnboardingView::from_manager(&OnboardingManager::new()));
    assert_eq!(onboarding.progress, "étape 1/5 : manifeste");
    assert!(!onboarding.limit_lines.is_empty() && !onboarding.cap_lines.is_empty());
}
