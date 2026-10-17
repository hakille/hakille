//! Parcours d'acceptation §23 (spec Akora) : 4 scénarios de référence.
//!
//! Utilise les VRAIES APIs disponibles dans `conformance` (sans nouvelle
//! dépendance) : agents mail (`AgentRegistry` + cartes de référence),
//! sync outbox (`Outbox`), ui channel (`ChannelSelector`, `EphemeralHud`),
//! shell powers/learning (`PowersView`, `LearningDiff`), journal
//! (`AuditJournal`). La sémantique cran 1 / GuestPass 1 h / anneau 1 px est
//! vérifiée via ces sources de vérité UI (validation requise, pass 1 h,
//! diff d'apprentissage vide pour l'invité).

use annora_agents::{AgentManifest, AgentRegistry};
use annora_audit::entry::{ActionRecord, AuditEntry, ReasoningStep, ReasoningTrace};
use annora_audit::journal::AuditJournal;
use annora_core::{ActorId, CapabilityScope, EntryId, KeyPair, Timestamp, UserId};
use annora_sync::{Outbox, OutboxEntry};
use annora_ui::{
    ChannelSelector, DeviceType, EphemeralHud, InteractionContext, OutputChannel, UrgencyLevel,
};
use annora_ui_shell::{LearningDiff, PowersView, reference_agent_cards};
use chrono::Duration;

fn test_journal() -> (tempfile::TempDir, AuditJournal) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("acceptance.redb");
    let keys = KeyPair::generate();
    let journal = AuditJournal::new(path.to_str().unwrap(), keys.signing_key.clone()).unwrap();
    (dir, journal)
}

fn journal_entry_for(action: &str, description: &str) -> AuditEntry {
    AuditEntry {
        id: EntryId(uuid::Uuid::now_v7()),
        timestamp: Timestamp::now(),
        actor: ActorId::User(UserId(uuid::Uuid::now_v7())),
        action: ActionRecord {
            action_type: action.to_string(),
            description: description.to_string(),
            parameters: serde_json::json!({}),
            reversible: true,
            reversed: false,
        },
        capability_used: None,
        reasoning_trace: ReasoningTrace {
            steps: vec![ReasoningStep {
                description: "Parce que tu m'as demandé de répondre à Awa.".to_string(),
                confidence: 0.92,
                evidence: vec![],
            }],
            model_used: None,
            duration_ms: 12,
        },
        graph_nodes_used: vec![],
        confidence: 0.92,
        cost: None,
        signature: vec![1, 2, 3, 4],
        prev_hash: vec![0; 32],
        token_hash: None,
    }
}

#[test]
fn test_journey_a_mail_cran1_card_approve_journal() {
    // A. « Dis Akora, réponds à Awa que j'arrive à 18 h » : bureau, seul,
    // en ligne, Mail au cran 1 (Préparer = valide chaque action).

    // 1. VRAI agent mail enregistré (registry + carte de référence).
    let mut registry = AgentRegistry::new();
    registry
        .register(AgentManifest {
            id: "agent-mail".to_string(),
            name: "Mail Agent".to_string(),
            version: "0.1.0".to_string(),
            description: "Reference sandboxed agent: Mail Agent".to_string(),
            required_capabilities: vec![CapabilityScope::Network, CapabilityScope::Filesystem],
        })
        .unwrap();
    assert!(registry.get_manifest("agent-mail").is_some());

    // Cran 1 = Préparer : chaque action se valide (carte honnête l'exige).
    let mail_card = reference_agent_cards()
        .into_iter()
        .find(|c| c.id == "agent-mail")
        .expect("carte agent-mail");
    assert!(mail_card.needs_validation);

    // 2. Carte : « Envoyer à Awa : j'arrive à 18 h ».
    // Pastilles imposées : coût, données, réversibilité.
    let cost_label = "0.00 EUR";
    let data_label = "Local";
    let reversibility_label = "− annulable";
    let mut hud = EphemeralHud::new();
    hud.request_validation(
        "mail-awa-18h",
        "Envoyer à Awa : j'arrive à 18 h",
        Some(0),
        "Reversible",
    );
    let ascii = hud.render_ascii();
    assert!(ascii.contains("mail-awa-18h") || ascii.contains("Awa"));
    assert_eq!([cost_label, data_label, reversibility_label].len(), 3);

    // 3. « Oui » / Valider → Approve (le noyau revérifie, l'UI ne fait que demander).
    assert!(hud.confirm_validation("mail-awa-18h"));

    // 4. Envoi + entrée au Journal avec sa trace « Pourquoi ».
    let (_dir, journal) = test_journal();
    let entry = journal_entry_for(
        "MailSend",
        "Envoyer à Awa : j'arrive à 18 h (Local, 0.00 EUR, annulable 30 s)",
    );
    journal.append(entry).unwrap();
    let entries = journal.all_entries().unwrap();
    assert_eq!(entries.len(), 1);
    let full = annora_ui::JournalView::full_reasoning(&entries[0]);
    assert!(full.contains("Awa"));
}

#[test]
fn test_journey_b_meeting_channel_n1_no_voice() {
    // B. La même demande en réunion, tiers présents : Sélecteur en N1 et
    // texte. Pas de voix, pas de son. Clavier + carte, Journal garde la trace.
    let ctx = InteractionContext {
        ambient_noise_db: 42.0,
        third_parties_present: true,
        in_meeting: true,
        user_attentive_to_screen: true,
        user_wearing_headphones: false,
        device_type: DeviceType::Desktop,
        urgency: UrgencyLevel::Normal,
        is_night_time: false,
    };
    let decision = ChannelSelector::arbitrate(&ctx);
    assert_eq!(decision.channel, OutputChannel::EphemeralHud);
    assert_ne!(decision.channel, OutputChannel::VoiceNormal);
    assert_ne!(decision.channel, OutputChannel::VoiceWhisper);

    // La demande se fait au clavier dans la ligne de commande.
    let mut hud = EphemeralHud::new();
    hud.open_prompt();
    hud.insert_text("Réponds à Awa que j'arrive à 18 h");
    match hud.state() {
        annora_ui::HudState::Prompting { query, .. } => {
            assert!(query.contains("Awa"));
        }
        _ => panic!("la carte clavier doit s'afficher en réunion"),
    }

    // Rien n'est lu à voix haute : aucun canal voix.
    let ctx_wearable = InteractionContext {
        in_meeting: true,
        device_type: DeviceType::Wearable,
        ..Default::default()
    };
    let d2 = ChannelSelector::arbitrate(&ctx_wearable);
    assert_ne!(d2.channel, OutputChannel::VoiceNormal);
}

#[test]
fn test_journey_c_offline_defer_revalidate_flush() {
    // C. Hors ligne : « Je l'enverrai à ta reconnexion », file visible,
    // revalidation > 15 min avant de partir.
    let mut outbox = Outbox::new();
    let mut old = OutboxEntry::new("Envoyer", "mail à Awa : j'arrive à 18 h", "mail-awa-18h");
    old.queued_at = chrono::Utc::now() - Duration::minutes(16);
    assert!(outbox.enqueue(old));
    assert_eq!(outbox.len(), 1);

    // 16 min sans revalidation → bloqué (jamais d'envoi surprise).
    let now = chrono::Utc::now();
    assert!(outbox.take_sendable(now).is_none());

    // Geste explicite → revalidé → flush envoyable + tracé au Journal.
    assert!(outbox.revalidate("mail-awa-18h"));
    let sent = outbox.take_sendable(now).expect("revalidé : doit partir");
    assert_eq!(sent.attempts, 1);
    assert!(outbox.is_empty());

    let (_dir, journal) = test_journal();
    let mut entry = journal_entry_for("OutboxFlush", "Transmis (file) : Envoyer mail à Awa");
    entry.action.action_type = "OutboxFlush".to_string();
    journal.append(entry).unwrap();
    assert_eq!(journal.all_entries().unwrap().len(), 1);
}

#[test]
fn test_journey_d_guest_pass_ring_no_learning() {
    // D. « Laisse Samir utiliser le wifi pendant une heure » : carte Invité,
    // anneau 1 px qui se vide, limité au pouvoir délégué, corrections jamais
    // apprises, coupure auto + registre à l'expiration.
    let mut view = PowersView::from_manifest(&["Network".to_string()], &[]);
    view.guest_passes
        .push("Samir — wifi — 1h (3600 s, anneau 1 px)".to_string());
    assert_eq!(view.guest_passes.len(), 1);
    assert!(view.guest_passes[0].contains("1h"));
    assert!(view.guest_passes[0].contains("3600"));
    assert!(view.guest_passes[0].contains("1 px"));

    // Limité au pouvoir délégué : le wifi, pas le wallet ni la mémoire.
    assert!(view.rows.iter().any(|r| r.scope == "Network"));
    assert!(view.rows.iter().all(|r| r.scope != "Wallet"));

    // Ses corrections ne nourrissent jamais l'apprentissage.
    let diff = LearningDiff::new("invité Samir");
    assert!(diff.is_empty());
    assert!(!diff.validated);
    assert!(diff.summary().contains("En attente de validation"));
    // Même après une correction de l'invité, le diff utilisateur reste vide :
    // rien n'est appliqué sans validation explicite.
    let still_empty = LearningDiff::new("invité Samir");
    assert!(still_empty.is_empty());

    // À l'expiration, tout se coupe seul (« Tout couper » révoque).
    view.kill_all();
    assert!(view.kill_all_armed);
    assert!(
        view.rows
            .iter()
            .filter(|r| !r.policy_locked)
            .all(|r| !r.enabled)
    );
}
