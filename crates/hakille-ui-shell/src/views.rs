//! View-models canoniques: Journal / Mémoire / Consentements / Onboarding / Coût.
//!
//! Pur Rust, testé sans fenêtre. Les renderers (desktop, daemon texte)
//! consomment ces VMs.

use hakille_ui::{JournalView, JournalViewItem, MemoryViewerItem};
use serde::{Deserialize, Serialize};

/// Ligne de journal pour la vue scroll Git-like.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalRow {
    pub id: String,
    pub time: String,
    pub actor: String,
    pub action: String,
    pub reason: String,
    /// Trace complète §19i (tous les steps, pas steps[0] seul).
    pub reason_full: String,
    /// Nœuds graphe ayant motivé l'action (triplet IDs).
    pub graph_nodes: Vec<String>,
    pub confidence_pct: u8,
    pub cost: String,
    pub verified: bool,
    pub can_revert: bool,
}

impl JournalRow {
    pub fn from_item(it: &JournalViewItem) -> Self {
        Self {
            id: it.entry_id.0.to_string(),
            time: it.timestamp_str.clone(),
            actor: it.actor_name.clone(),
            action: it.action_summary.clone(),
            reason: it.reasoning_summary.clone(),
            reason_full: it.reasoning_summary.clone(),
            graph_nodes: Vec::new(),
            confidence_pct: it.confidence_percentage,
            cost: it
                .cost_cents
                .map(|c| format!("{:.2} EUR", c as f64 / 100.0))
                .unwrap_or_default(),
            verified: it.signature_verified,
            can_revert: it.can_revert,
        }
    }

    /// Construit depuis l'entrée brute : trace complète + nœuds graphe.
    pub fn from_entry(entry: &hakille_audit::entry::AuditEntry) -> Self {
        let item = JournalView::format_entry(entry);
        let mut row = Self::from_item(&item);
        row.reason_full = JournalView::full_reasoning(entry);
        row.graph_nodes = JournalView::graph_node_labels(entry);
        row
    }

    /// Recherche exacte insensible à la casse (le ranking sémantique HNSW
    /// reste côté graphe ; ici on filtre l'affichage).
    pub fn matches(&self, q: &str) -> bool {
        let q = q.to_lowercase();
        q.is_empty()
            || self.action.to_lowercase().contains(&q)
            || self.reason.to_lowercase().contains(&q)
            || self.reason_full.to_lowercase().contains(&q)
            || self.actor.to_lowercase().contains(&q)
    }
}

/// Filtre + tri antéchronologique (time RFC3339 triable lexicographiquement)
/// + pagination pour la vue scroll Git-like (1000+ entrées).
///
/// Perfs (pas de logique métier ici) : `filter_journal` est O(n) filtre +
/// O(k log k) tri sur les k lignes filtrées, `page_journal` ne copie qu'une
/// fenêtre de `per_page` références (liste virtualisée, typ. 20 lignes).
/// Budget CI : 100 000 entrées filtrées + paginées en < 2 s en debug
/// (voir `test_journal_100k_filter_and_page_budget`). Le renderer ne
/// reçoit que la page visible, jamais les 100k lignes.
pub fn filter_journal<'a>(rows: &'a [JournalRow], query: &str) -> Vec<&'a JournalRow> {
    let mut out: Vec<&JournalRow> = rows.iter().filter(|r| r.matches(query)).collect();
    out.sort_by(|a, b| b.time.cmp(&a.time));
    out
}

/// Page 0-indexed de `per_page` lignes (défense : per_page==0 → vide).
pub fn page_journal<'a>(
    rows: &[&'a JournalRow],
    page: usize,
    per_page: usize,
) -> Vec<&'a JournalRow> {
    if per_page == 0 {
        return Vec::new();
    }
    rows.iter()
        .skip(page.saturating_mul(per_page))
        .take(per_page)
        .copied()
        .collect()
}

/// Frise groupée par jour (spec §8) : `YYYY-MM-DD` → lignes, jours décroissants.
/// Les horodatages RFC3339 se comparent lexicographiquement.
pub fn group_journal_by_day<'a>(rows: &[&'a JournalRow]) -> Vec<(String, Vec<&'a JournalRow>)> {
    use std::collections::BTreeMap;
    let mut days: BTreeMap<String, Vec<&'a JournalRow>> = BTreeMap::new();
    for r in rows {
        let day = r.time.chars().take(10).collect::<String>();
        days.entry(day).or_default().push(*r);
    }
    let mut out: Vec<(String, Vec<&'a JournalRow>)> = days.into_iter().collect();
    out.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, group) in &mut out {
        group.sort_by(|a, b| b.time.cmp(&a.time));
    }
    out
}

/// Diff d'apprentissage (spec §8, §20f) : « voici ce que j'ai appris cette
/// semaine — tu valides ? » Jamais appliqué sans validation explicite.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LearningDiff {
    pub period: String,
    pub additions: Vec<String>,
    pub corrections: Vec<String>,
    pub validated: bool,
}

impl LearningDiff {
    pub fn new(period: &str) -> Self {
        Self {
            period: period.to_string(),
            additions: vec![],
            corrections: vec![],
            validated: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.additions.is_empty() && self.corrections.is_empty()
    }

    pub fn validate(&mut self) {
        self.validated = true;
    }

    pub fn summary(&self) -> String {
        format!(
            "Appris ({}): +{} ajouts, ~{} corrections. {}",
            self.period,
            self.additions.len(),
            self.corrections.len(),
            if self.validated {
                "Validé par l'utilisateur."
            } else {
                "En attente de validation — rien n'est appliqué."
            }
        )
    }
}

/// Ligne mémoire pour le panneau triplets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRow {
    pub id: String,
    pub text: String,
    pub weight: f64,
    pub confidence_pct: u8,
    pub source: String,
    pub partition: String,
}

impl MemoryRow {
    pub fn from_item(it: &MemoryViewerItem) -> Self {
        Self {
            id: it.id.0.to_string(),
            text: it.text_representation.clone(),
            weight: it.weight,
            confidence_pct: it.confidence_pct,
            source: it.source.clone(),
            partition: it.partition.clone(),
        }
    }

    /// Indicateur de fraîcheur (spec §8) depuis le poids à décroissance.
    pub fn freshness_label(weight: f64) -> &'static str {
        if weight >= 0.8 {
            "frais"
        } else if weight >= 0.4 {
            "récent"
        } else {
            "fané"
        }
    }

    pub fn freshness(&self) -> &'static str {
        Self::freshness_label(self.weight)
    }
}

/// Bascule pro/perso (spec §8, §21) : filtre les lignes par partition,
/// sans jamais fusionner partagé et privé automatiquement.
pub fn filter_by_partition<'a>(rows: &'a [MemoryRow], partition: &str) -> Vec<&'a MemoryRow> {
    rows.iter().filter(|r| r.partition == partition).collect()
}

/// État onboarding pour le premier écran "limites avant capacités".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingView {
    pub limits: Vec<(String, String, bool)>,
    pub capabilities: Vec<String>,
    pub acknowledged: bool,
    pub ready: bool,
}

impl OnboardingView {
    pub fn from_manager(m: &hakille_ui::OnboardingManager) -> Self {
        let man = m.manifest();
        Self {
            limits: man
                .guaranteed_limits
                .iter()
                .map(|l| (l.title.clone(), l.description.clone(), l.non_negotiable))
                .collect(),
            capabilities: man.granted_capabilities.clone(),
            acknowledged: man.acknowledged,
            ready: m.is_ready(),
        }
    }
}

/// Pouvoir (spec §8) : capacité groupée avec périmètre, quota, dernier
/// usage, expiration, interrupteur. Registre de consentement en frise.
/// Bouton « Tout couper » : révoque tout en un geste.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerRow {
    /// Groupe : Voir, Entendre, Parler, Agir, Dépenser, Partager.
    pub group: String,
    pub scope: String,
    pub perimeter: String,
    pub quota_label: String,
    pub quota_ratio: f32,
    pub last_used: String,
    pub expires: String,
    pub enabled: bool,
    /// Verrouillé par politique (distinct de désactivé).
    pub policy_locked: bool,
}

impl PowerRow {
    /// Jauge : normale / au-delà de 80 % / atteinte. Valeur toujours textuelle.
    pub fn gauge_state(&self) -> &'static str {
        if self.quota_ratio >= 1.0 {
            "atteinte"
        } else if self.quota_ratio >= 0.8 {
            "au-delà de 80 %"
        } else {
            "normale"
        }
    }
}

/// Écran Pouvoirs : lignes par groupe + invités temporaires + kill switch.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PowersView {
    pub rows: Vec<PowerRow>,
    pub guest_passes: Vec<String>,
    pub kill_all_armed: bool,
}

impl PowersView {
    /// Construit depuis le manifeste plateforme : accordées → activées,
    /// refusées → verrouillées par politique.
    pub fn from_manifest(granted: &[String], denied: &[String]) -> Self {
        let group_of = |scope: &str| match scope {
            "Camera" => "Voir",
            "Audio" => "Entendre",
            "GuiInput" => "Agir",
            "Wallet" => "Dépenser",
            "Network" => "Partager",
            _ => "Agir",
        };
        let mut rows: Vec<PowerRow> = granted
            .iter()
            .map(|s| PowerRow {
                group: group_of(s).to_string(),
                scope: s.clone(),
                perimeter: "bac à sable VirtualFS / allowlist".to_string(),
                quota_label: "quota suivi au point d'effet".to_string(),
                quota_ratio: 0.0,
                last_used: "—".to_string(),
                expires: "session".to_string(),
                enabled: true,
                policy_locked: false,
            })
            .collect();
        for s in denied {
            rows.push(PowerRow {
                group: group_of(s).to_string(),
                scope: s.clone(),
                perimeter: "refusé par la plateforme".to_string(),
                quota_label: "0".to_string(),
                quota_ratio: 0.0,
                last_used: "jamais".to_string(),
                expires: "—".to_string(),
                enabled: false,
                policy_locked: true,
            });
        }
        Self {
            rows,
            guest_passes: vec![],
            kill_all_armed: false,
        }
    }

    /// « Tout couper » : désactive tout ce qui n'est pas verrouillé.
    pub fn kill_all(&mut self) {
        for r in &mut self.rows {
            if !r.policy_locked {
                r.enabled = false;
            }
        }
        self.kill_all_armed = true;
    }
}

/// Fournisseur d'inférence + pastille « où vont mes données » (spec §8).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRow {
    pub name: String,
    /// "Local" ou "Cloud : <fournisseur> (≈ poids)".
    pub data_label: String,
    pub is_local: bool,
}

impl ProviderRow {
    pub fn local(name: &str) -> Self {
        Self {
            name: name.to_string(),
            data_label: "Local".to_string(),
            is_local: true,
        }
    }

    pub fn cloud(name: &str, provider: &str) -> Self {
        Self {
            name: name.to_string(),
            data_label: format!("Cloud : {provider}"),
            is_local: false,
        }
    }
}

/// Écran Réglages (spec §8) : pur view-model, sans logique métier.
/// Fournisseurs + voix + canaux + langues + accessibilité + sauvegarde +
/// foyer + simulation. Les renderers consomment ces lignes ; `:settings`
/// les affiche en texte.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsView {
    pub providers: Vec<ProviderRow>,
    pub voices: Vec<String>,
    pub channels: Vec<String>,
    pub languages: Vec<String>,
    pub accessibility_summary: String,
    pub backup_label: String,
    pub household_profiles: Vec<String>,
    /// Mode simulation activé par défaut (onboarding) : rien ne s'exécute.
    pub simulation_mode: bool,
}

impl Default for SettingsView {
    fn default() -> Self {
        Self {
            providers: vec![
                ProviderRow::local("Ollama (local)"),
                ProviderRow::cloud("OpenAI", "OpenAI"),
                ProviderRow::cloud("Anthropic", "Anthropic"),
                ProviderRow::cloud("Personnalisé", "endpoint choisi"),
            ],
            voices: vec![
                "Voix utilitaire neutre".to_string(),
                "Voix utilitaire chaleureuse sobre".to_string(),
                "Voix utilitaire grave".to_string(),
            ],
            channels: vec![
                "VoiceNormal".to_string(),
                "VoiceWhisper".to_string(),
                "EphemeralHud".to_string(),
                "Haptic".to_string(),
                "AmbientPulse".to_string(),
                "Defer".to_string(),
            ],
            languages: vec![
                "fr".to_string(),
                "en".to_string(),
                "ar".to_string(),
                "ff-Adlm".to_string(),
                "nqo-Nkoo".to_string(),
            ],
            accessibility_summary: "VoiceFirst par défaut ; TextFirst/HapticAssisted reconfigurent le canal, jamais dégradé"
                .to_string(),
            backup_label: "Sauvegarde locale chiffrée, export en un geste".to_string(),
            household_profiles: vec![
                "Personnel".to_string(),
                "Professionnel".to_string(),
                "Foyer".to_string(),
            ],
            simulation_mode: true,
        }
    }
}

impl SettingsView {
    pub fn summary(&self) -> String {
        let providers = self
            .providers
            .iter()
            .map(|p| format!("{} [{}]", p.name, p.data_label))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "Réglages §8 — fournisseurs : {} | voix : {} | canaux : {} | langues : {} | accessibilité : {} | sauvegarde : {} | foyer : {} | simulation : {}",
            providers,
            self.voices.join(", "),
            self.channels.join(", "),
            self.languages.join(", "),
            self.accessibility_summary,
            self.backup_label,
            self.household_profiles.join(", "),
            if self.simulation_mode {
                "activée (prévisualisation, rien ne s'exécute)"
            } else {
                "désactivée"
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_journal_row_search_is_case_insensitive() {
        let row = JournalRow {
            id: "1".into(),
            time: "t".into(),
            actor: "System".into(),
            action: "Envoi 25.00 EUR validé".into(),
            reason: "Graphe + confiance 0.91".into(),
            reason_full: "1. step one\n2. step two".into(),
            graph_nodes: vec![],
            confidence_pct: 91,
            cost: "25.00 EUR".into(),
            verified: true,
            can_revert: true,
        };
        assert!(row.matches(""));
        assert!(row.matches("envoi"));
        assert!(row.matches("CONFIANCE"));
        assert!(row.matches("step two")); // trace complète cherchable
        assert!(!row.matches("supprimer"));
    }

    #[test]
    fn test_onboarding_view_blocks_until_ack() {
        let mut m = hakille_ui::OnboardingManager::new();
        let v0 = OnboardingView::from_manager(&m);
        assert!(!v0.ready);
        assert_eq!(v0.limits.len(), 4);
        assert_eq!(v0.capabilities.len(), 4);
        m.acknowledge_limits();
        let v1 = OnboardingView::from_manager(&m);
        assert!(v1.ready);
    }

    fn sample_row(id: &str, time: &str, action: &str) -> JournalRow {
        JournalRow {
            id: id.into(),
            time: time.into(),
            actor: "System".into(),
            action: action.into(),
            reason: "r".into(),
            reason_full: "r".into(),
            graph_nodes: vec![],
            confidence_pct: 90,
            cost: String::new(),
            verified: true,
            can_revert: true,
        }
    }

    #[test]
    fn test_filter_sorts_newest_first_and_filters() {
        let rows = vec![
            sample_row("1", "2026-10-11T10:00:00Z", "Envoi EUR"),
            sample_row("2", "2026-10-11T12:00:00Z", "Lecture mail"),
            sample_row("3", "2026-10-11T11:00:00Z", "Envoi EUR bis"),
        ];
        let all = filter_journal(&rows, "");
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].id, "2"); // plus récent d'abord
        assert_eq!(all[2].id, "1");
        let envois = filter_journal(&rows, "envoi");
        assert_eq!(envois.len(), 2);
        assert_eq!(envois[0].id, "3");
    }

    #[test]
    fn test_page_journal_paginates() {
        let rows = vec![
            sample_row("1", "2026-10-11T10:00:00Z", "a"),
            sample_row("2", "2026-10-11T11:00:00Z", "b"),
            sample_row("3", "2026-10-11T12:00:00Z", "c"),
        ];
        let all = filter_journal(&rows, "");
        let p0 = page_journal(&all, 0, 2);
        let p1 = page_journal(&all, 1, 2);
        let p2 = page_journal(&all, 2, 2);
        assert_eq!(p0.len(), 2);
        assert_eq!(p1.len(), 1);
        assert!(p2.is_empty());
        assert!(page_journal(&all, 0, 0).is_empty());
    }

    #[test]
    fn test_powers_kill_all_respects_policy_lock() {
        let granted = vec!["Camera".to_string(), "Wallet".to_string()];
        let denied = vec!["FullSystem".to_string()];
        let mut view = PowersView::from_manifest(&granted, &denied);
        assert_eq!(view.rows.len(), 3);
        assert_eq!(view.rows[0].group, "Voir");
        assert_eq!(view.rows[0].gauge_state(), "normale");
        let locked = view.rows.iter().find(|r| r.policy_locked).unwrap();
        assert!(!locked.enabled);
        view.kill_all();
        assert!(view.kill_all_armed);
        assert!(
            view.rows
                .iter()
                .filter(|r| !r.policy_locked)
                .all(|r| !r.enabled)
        );
    }

    #[test]
    fn test_from_entry_carries_full_trace_and_nodes() {
        use hakille_core::{ActorId, EntryId, Timestamp, TripletId, UserId};
        let node = TripletId(uuid::Uuid::now_v7());
        let entry = hakille_audit::entry::AuditEntry {
            id: EntryId(uuid::Uuid::now_v7()),
            timestamp: Timestamp::now(),
            actor: ActorId::User(UserId(uuid::Uuid::now_v7())),
            action: hakille_audit::entry::ActionRecord {
                action_type: "send".into(),
                description: "d".into(),
                parameters: serde_json::json!({}),
                reversible: true,
                reversed: false,
            },
            capability_used: None,
            reasoning_trace: hakille_audit::entry::ReasoningTrace {
                steps: vec![
                    hakille_audit::entry::ReasoningStep {
                        description: "alpha".into(),
                        confidence: 0.9,
                        evidence: vec![],
                    },
                    hakille_audit::entry::ReasoningStep {
                        description: "beta".into(),
                        confidence: 0.8,
                        evidence: vec![],
                    },
                ],
                model_used: None,
                duration_ms: 1,
            },
            graph_nodes_used: vec![node],
            confidence: 0.9,
            cost: None,
            signature: vec![],
            prev_hash: vec![],
            token_hash: None,
        };
        let row = JournalRow::from_entry(&entry);
        assert!(row.reason_full.contains("alpha") && row.reason_full.contains("beta"));
        assert_eq!(row.graph_nodes.len(), 1);
    }

    #[test]
    fn test_group_journal_by_day_descending() {
        let rows = [
            sample_row("1", "2026-10-10T10:00:00Z", "a"),
            sample_row("2", "2026-10-11T12:00:00Z", "b"),
            sample_row("3", "2026-10-11T09:00:00Z", "c"),
        ];
        let refs: Vec<&JournalRow> = rows.iter().collect();
        let groups = group_journal_by_day(&refs);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].0, "2026-10-11");
        assert_eq!(groups[0].1.len(), 2);
        assert_eq!(groups[0].1[0].id, "2"); // plus récent du jour d'abord
        assert_eq!(groups[1].0, "2026-10-10");
    }

    #[test]
    fn test_learning_diff_needs_explicit_validation() {
        let mut diff = LearningDiff::new("semaine 42");
        assert!(diff.is_empty());
        diff.additions.push("Awa préfère être tutoyée".into());
        assert!(!diff.validated);
        assert!(diff.summary().contains("En attente de validation"));
        diff.validate();
        assert!(diff.validated);
        assert!(diff.summary().contains("Validé"));
    }

    fn sample_memory_row(partition: &str, weight: f64) -> MemoryRow {
        MemoryRow {
            id: "x".into(),
            text: "t".into(),
            weight,
            confidence_pct: 90,
            source: "User".into(),
            partition: partition.into(),
        }
    }

    #[test]
    fn test_memory_freshness_and_partition_toggle() {
        assert_eq!(MemoryRow::freshness_label(0.9), "frais");
        assert_eq!(MemoryRow::freshness_label(0.5), "récent");
        assert_eq!(MemoryRow::freshness_label(0.1), "fané");
        let rows = vec![
            sample_memory_row("Personal", 0.9),
            sample_memory_row("Professional", 0.5),
            sample_memory_row("Personal", 0.2),
        ];
        let perso = filter_by_partition(&rows, "Personal");
        assert_eq!(perso.len(), 2);
        assert_eq!(perso[0].freshness(), "frais");
        assert_eq!(perso[1].freshness(), "fané");
        assert!(filter_by_partition(&rows, "Shared").is_empty());
    }

    #[test]
    fn test_settings_view_covers_section8() {
        let v = SettingsView::default();
        // Fournisseurs avec pastille Local/Cloud.
        assert!(v.providers.iter().any(|p| p.data_label == "Local"));
        assert!(
            v.providers
                .iter()
                .any(|p| p.data_label.starts_with("Cloud :"))
        );
        assert!(v.voices.len() >= 2);
        assert!(v.channels.contains(&"EphemeralHud".to_string()));
        assert!(v.languages.contains(&"fr".to_string()));
        assert!(!v.accessibility_summary.is_empty());
        assert!(!v.backup_label.is_empty());
        assert!(v.household_profiles.contains(&"Foyer".to_string()));
        assert!(v.simulation_mode);
        let s = v.summary();
        assert!(s.contains("Local"));
        assert!(s.contains("simulation"));
    }

    #[test]
    fn test_journal_100k_filter_and_page_budget() {
        // Spec §11 : 100 000 entrées, liste virtualisée. Le filtre + tri +
        // pagination (fenêtre de 20) doivent rester sous 2 s en debug.
        let rows: Vec<JournalRow> = (0..100_000)
            .map(|i| {
                sample_row(
                    &i.to_string(),
                    &format!("2026-10-{:02}T10:00:00Z", 1 + (i % 28)),
                    if i % 97 == 0 {
                        "Envoi EUR"
                    } else {
                        "Lecture mail"
                    },
                )
            })
            .collect();
        let start = std::time::Instant::now();
        let filtered = filter_journal(&rows, "envoi");
        let page = page_journal(&filtered, 0, 20);
        let elapsed = start.elapsed();
        assert_eq!(page.len(), 20);
        assert!(filtered.len() > 1000);
        assert!(elapsed.as_secs() < 2, "budget perf dépassé : {elapsed:?}");
    }

    #[test]
    fn test_snapshot_and_channel_selector_perf_budget() {
        // Chemins réels : ShellSnapshot vit dans crate::bridge (ré-exporté
        // racine), ChannelSelector + InteractionContext dans hakille_ui.
        use crate::bridge::ShellSnapshot;
        use hakille_ui::{ChannelSelector, CostCounter, EphemeralHud, InteractionContext};
        use hakille_ui::{PresenceIndicator, SilenceBudgetTracker};

        let presence = PresenceIndicator::new();
        let hud = EphemeralHud::new();
        let cost = CostCounter::new(500, "EUR");
        let silence = SilenceBudgetTracker::new(3);

        let start = std::time::Instant::now();
        for i in 0..1000 {
            let _ = ShellSnapshot::from_states(&presence, &hud, &cost, &silence, i, 0);
        }
        let elapsed = start.elapsed();
        assert!(elapsed.as_secs() < 2, "budget perf dépassé : {elapsed:?}");

        let ctx = InteractionContext::default();
        let start = std::time::Instant::now();
        for _ in 0..10_000 {
            let _ = ChannelSelector::arbitrate(&ctx);
        }
        let elapsed = start.elapsed();
        assert!(elapsed.as_secs() < 2, "budget perf dépassé : {elapsed:?}");
    }
}

// ===== ÉQUIPE MODEL — écrans §8 (ajouts additifs, VMs 100 % pures, zéro dépendance renderer) =====
//
// Notes d'intégration (lues sur le code exact, pas d'hypothèse) :
// - `hakille-sync` et `hakille-design` NE SONT PAS en dépendances d'`hakille-ui-shell`
//   (ajout interdit) : `OutboxPanelVM::from_entries` prend donc des triplets simples
//   `(clé, "verbe objet", statut)` — le daemon mappe `OutboxEntry` via
//   `sendable(now)` / `waiting_minutes(now)` (voir `crates/hakille-sync/src/outbox.rs`).
// - `ActionCardStrings::from_parts` est le miroir champ à champ de
//   `hakille_design::ActionCardView` (`crates/hakille-design/src/action_card.rs`) :
//   le renderer desktop appelle `from_parts(card.verb, card.object, card.reason,
//   card.cost_label, &card.data.label(), card.reversibility.label(),
//   card.needs_hold, &card.provenance_label, card.is_external)`.
//   Pastille externe canonique : `ExternalBlock::badge()` = `"externe"`.
//   Appui long canonique : 600 ms (`hakille_design::hold`, `HOLD_TO_CONFIRM_MS`).
// - `WhyStrings::from_parts` est le miroir de `hakille_design::WhySheet`
//   (`crates/hakille-design/src/why.rs`) : `facts` = `(label, kind, date, source)`.
//   Seuils de confiance recopiés de `WhySheet::confidence_word`
//   (< 0.5 basse, < 0.85 moyenne, sinon haute) — garder en sync si design change.

/// Écran Mémoire §8 : lignes formatées (textes + partitions + contradictions).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryScreenVM {
    /// `"[fraîcheur] texte (partition, conf%)"`, filtré par partition (`"Tout"` = tout).
    pub visible: Vec<String>,
    /// Toujours `["Tout", "Personal", "Professional", "Shared"]`.
    pub partitions: Vec<String>,
    /// `"X contradiction(s) — Garder A / B / les deux"`.
    pub contra_label: String,
}

impl MemoryScreenVM {
    pub fn partition_list() -> Vec<String> {
        ["Tout", "Personal", "Professional", "Shared"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    pub fn from_rows(rows: &[MemoryRow], partition: &str, contra_count: usize) -> Self {
        let visible = rows
            .iter()
            .filter(|r| partition == "Tout" || r.partition == partition)
            .map(|r| {
                format!(
                    "[{}] {} ({}, {}%)",
                    r.freshness(),
                    r.text,
                    r.partition,
                    r.confidence_pct
                )
            })
            .collect();
        Self {
            visible,
            partitions: Self::partition_list(),
            contra_label: format!("{contra_count} contradiction(s) — Garder A / B / les deux"),
        }
    }
}

/// Écran Réglages §8 en lignes texte (pastilles, voix, canaux, langues, a11y…).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsState {
    pub rows: Vec<String>,
}

impl SettingsState {
    pub fn from_settings(s: &SettingsView) -> Self {
        let mut rows: Vec<String> = s
            .providers
            .iter()
            .map(|p| format!("Fournisseur : {} [{}]", p.name, p.data_label))
            .collect();
        rows.push(format!("Voix : {}", s.voices.join(", ")));
        rows.push(format!("Canaux : {}", s.channels.join(", ")));
        rows.push(format!("Langues : {}", s.languages.join(", ")));
        rows.push(format!("Accessibilité : {}", s.accessibility_summary));
        rows.push(format!("Sauvegarde : {}", s.backup_label));
        rows.push(format!("Foyer : {}", s.household_profiles.join(", ")));
        rows.push(format!(
            "Simulation : {}",
            if s.simulation_mode {
                "activée (prévisualisation, rien ne s'exécute)"
            } else {
                "désactivée"
            }
        ));
        Self { rows }
    }
}

/// Feuille onboarding : limites puces + capacités + progression fixe étape 1/5.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingSheetVM {
    pub limit_lines: Vec<String>,
    pub cap_lines: Vec<String>,
    pub progress: String,
}

impl OnboardingSheetVM {
    pub fn from_view(v: &OnboardingView) -> Self {
        Self {
            limit_lines: v
                .limits
                .iter()
                .map(|(title, desc, non_neg)| {
                    format!(
                        "• {} [{}] — {}",
                        title,
                        if *non_neg {
                            "NON-NÉGOCIABLE"
                        } else {
                            "négociable"
                        },
                        desc
                    )
                })
                .collect(),
            cap_lines: v.capabilities.iter().map(|c| format!("+ {c}")).collect(),
            progress: "étape 1/5 : manifeste".to_string(),
        }
    }
}

/// File d'envoi visible §8/§19 : une ligne texte par entrée.
/// Triplet d'entrée : `(clé_dedup, "verbe objet", statut)` — statut fourni par le
/// daemon (`"envoyable"` ou `"revalidation requise (> 15 min)"`).
pub struct OutboxPanelVM;

impl OutboxPanelVM {
    pub fn from_entries(rows: &[(String, String, String)]) -> Vec<String> {
        rows.iter()
            .map(|(key, verb_object, status)| format!("[{key}] {verb_object} — {status}"))
            .collect()
    }
}

/// Carte d'action en strings (une seule « décision requise » à l'écran côté render).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionCardStrings {
    pub title: String,
    pub reason: String,
    /// Ordre imposé : coût, données, réversibilité.
    pub pastilles: [String; 3],
    pub validate_label: String,
    pub provenance: String,
    /// `Some("externe")` si contenu externe, sinon `None`.
    pub external_badge: Option<String>,
}

impl ActionCardStrings {
    #[allow(clippy::too_many_arguments)]
    pub fn from_parts(
        verb: &str,
        object: &str,
        reason: &str,
        cost_label: &str,
        data_label: &str,
        reversibility_label: &str,
        needs_hold: bool,
        provenance_label: &str,
        is_external: bool,
    ) -> Self {
        Self {
            title: format!("{verb} {object}"),
            reason: reason.to_string(),
            pastilles: [
                cost_label.to_string(),
                data_label.to_string(),
                reversibility_label.to_string(),
            ],
            validate_label: if needs_hold {
                "Valider (appui long 600 ms)".to_string()
            } else {
                "Valider".to_string()
            },
            provenance: provenance_label.to_string(),
            external_badge: if is_external {
                Some("externe".to_string())
            } else {
                None
            },
        }
    }
}

/// Feuille « Pourquoi » 3 niveaux en strings (spec §25).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhyStrings {
    pub sentence: String,
    /// `"label — kind, date, source"` par fait.
    pub facts: Vec<String>,
    pub plan: String,
    pub powers: String,
    /// `"haute (0.92)"` : mot de confiance + valeur.
    pub confidence: String,
    pub alternatives: Vec<String>,
    pub model_line: String,
    pub cost_line: String,
    pub data_line: String,
    pub would_change: String,
}

impl WhyStrings {
    /// Seuils recopiés de `hakille_design::WhySheet::confidence_word`
    /// (< 0.5 basse, < 0.85 moyenne, sinon haute).
    pub fn confidence_word(value: f64) -> &'static str {
        if value < 0.5 {
            "basse"
        } else if value < 0.85 {
            "moyenne"
        } else {
            "haute"
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_parts(
        sentence: &str,
        facts: &[(String, String, String, String)],
        plan: &str,
        powers: &[String],
        confidence_value: f64,
        alternatives: &[String],
        model_line: &str,
        cost_line: &str,
        data_line: &str,
        would_change: &str,
    ) -> Self {
        Self {
            sentence: sentence.to_string(),
            facts: facts
                .iter()
                .map(|(label, kind, date, source)| format!("{label} — {kind}, {date}, {source}"))
                .collect(),
            plan: plan.to_string(),
            powers: powers.join(", "),
            confidence: format!(
                "{} ({confidence_value:.2})",
                Self::confidence_word(confidence_value)
            ),
            alternatives: alternatives.to_vec(),
            model_line: model_line.to_string(),
            cost_line: cost_line.to_string(),
            data_line: data_line.to_string(),
            would_change: would_change.to_string(),
        }
    }

    /// Miroir de `WhySheet::missing` : la feuille dit quand elle ne sait pas.
    pub fn missing(reason: &str) -> Self {
        Self {
            sentence: "Je n'ai pas gardé la raison de ce choix.".to_string(),
            facts: vec![],
            plan: reason.to_string(),
            powers: String::new(),
            confidence: format!("{} ({:.2})", Self::confidence_word(0.0), 0.0),
            alternatives: vec![],
            model_line: "inconnu".to_string(),
            cost_line: "inconnu".to_string(),
            data_line: "inconnu".to_string(),
            would_change: "inconnu".to_string(),
        }
    }
}

#[cfg(test)]
mod screens_tests {
    use super::*;

    fn mem_row(text: &str, weight: f64, conf: u8, partition: &str) -> MemoryRow {
        MemoryRow {
            id: "id".into(),
            text: text.into(),
            weight,
            confidence_pct: conf,
            source: "User".into(),
            partition: partition.into(),
        }
    }

    #[test]
    fn test_memory_screen_vm_format_and_partitions() {
        let rows = vec![
            mem_row("Marie aime le jazz", 0.9, 90, "Personal"),
            mem_row("Réunion lundi 10h", 0.5, 70, "Professional"),
        ];
        let vm = MemoryScreenVM::from_rows(&rows, "Tout", 2);
        assert_eq!(
            vm.visible,
            vec![
                "[frais] Marie aime le jazz (Personal, 90%)".to_string(),
                "[récent] Réunion lundi 10h (Professional, 70%)".to_string(),
            ]
        );
        assert_eq!(
            vm.partitions,
            vec!["Tout", "Personal", "Professional", "Shared"]
        );
        assert_eq!(
            vm.contra_label,
            "2 contradiction(s) — Garder A / B / les deux"
        );
    }

    #[test]
    fn test_memory_screen_vm_partition_filters_without_merging() {
        let rows = vec![
            mem_row("perso", 0.9, 90, "Personal"),
            mem_row("pro", 0.9, 90, "Professional"),
        ];
        let perso = MemoryScreenVM::from_rows(&rows, "Personal", 0);
        assert_eq!(
            perso.visible,
            vec!["[frais] perso (Personal, 90%)".to_string()]
        );
        let pro = MemoryScreenVM::from_rows(&rows, "Professional", 0);
        assert_eq!(
            pro.visible,
            vec!["[frais] pro (Professional, 90%)".to_string()]
        );
        // Jamais de fusion : chaque filtre ne voit que sa partition.
        assert!(!perso.visible.iter().any(|l| l.contains("Professional")));
        assert!(!pro.visible.iter().any(|l| l.contains("Personal")));
    }

    #[test]
    fn test_memory_screen_vm_empty() {
        let vm = MemoryScreenVM::from_rows(&[], "Tout", 0);
        assert!(vm.visible.is_empty());
        assert_eq!(vm.partitions.len(), 4);
        assert_eq!(
            vm.contra_label,
            "0 contradiction(s) — Garder A / B / les deux"
        );
    }

    #[test]
    fn test_settings_state_rows_cover_section8() {
        let state = SettingsState::from_settings(&SettingsView::default());
        assert!(
            state
                .rows
                .iter()
                .any(|r| r == "Fournisseur : Ollama (local) [Local]")
        );
        assert!(
            state
                .rows
                .iter()
                .any(|r| r.starts_with("Fournisseur : OpenAI [Cloud :"))
        );
        assert!(state.rows.iter().any(|r| r.starts_with("Voix : ")));
        assert!(state.rows.iter().any(|r| r.contains("EphemeralHud")));
        assert!(state.rows.iter().any(|r| r.starts_with("Langues : ")));
        assert!(state.rows.iter().any(|r| r.starts_with("Accessibilité : ")));
        assert!(state.rows.iter().any(|r| r.starts_with("Sauvegarde : ")));
        assert!(state.rows.iter().any(|r| r.starts_with("Foyer : ")));
        assert!(
            state
                .rows
                .iter()
                .any(|r| r == "Simulation : activée (prévisualisation, rien ne s'exécute)")
        );
    }

    #[test]
    fn test_onboarding_sheet_vm_progress_and_lines() {
        let view = OnboardingView::from_manager(&hakille_ui::OnboardingManager::new());
        let vm = OnboardingSheetVM::from_view(&view);
        assert_eq!(vm.progress, "étape 1/5 : manifeste");
        assert_eq!(vm.limit_lines.len(), 4);
        assert!(vm.limit_lines[0].starts_with("• "));
        assert!(vm.limit_lines[0].contains("[NON-NÉGOCIABLE]"));
        assert_eq!(vm.cap_lines.len(), 4);
        assert!(vm.cap_lines.iter().all(|l| l.starts_with("+ ")));
    }

    #[test]
    fn test_outbox_panel_vm_lines_and_empty() {
        let lines = OutboxPanelVM::from_entries(&[
            (
                "mail-1".into(),
                "Envoyer mail à Awa".into(),
                "envoyable".into(),
            ),
            (
                "sms-9".into(),
                "Envoyer sms à Moussa".into(),
                "revalidation requise (> 15 min)".into(),
            ),
        ]);
        assert_eq!(
            lines,
            vec![
                "[mail-1] Envoyer mail à Awa — envoyable".to_string(),
                "[sms-9] Envoyer sms à Moussa — revalidation requise (> 15 min)".to_string(),
            ]
        );
        assert!(OutboxPanelVM::from_entries(&[]).is_empty());
    }

    #[test]
    fn test_action_card_strings_mail_first_notch() {
        // Cran 1 mail : pastilles Local / 0 € / annulable, pas d'appui long.
        let s = ActionCardStrings::from_parts(
            "Envoyer",
            "ce mail à Awa",
            "Tu as demandé de répondre.",
            "0 €",
            "Local",
            "− annulable",
            false,
            "Hakille",
            false,
        );
        assert_eq!(s.title, "Envoyer ce mail à Awa");
        assert_eq!(
            s.pastilles,
            [
                "0 €".to_string(),
                "Local".to_string(),
                "− annulable".to_string()
            ]
        );
        assert_eq!(s.validate_label, "Valider");
        assert_eq!(s.provenance, "Hakille");
        assert_eq!(s.external_badge, None);
    }

    #[test]
    fn test_action_card_strings_hold_and_external() {
        let s = ActionCardStrings::from_parts(
            "Virer",
            "25,00 EUR au restaurant",
            "Ordre vocal confirmé.",
            "0,01 €",
            "Cloud : OpenAI (≈ 40 Ko)",
            "+ définitif",
            true,
            "externe",
            true,
        );
        assert_eq!(s.validate_label, "Valider (appui long 600 ms)");
        assert_eq!(s.external_badge, Some("externe".to_string()));
        assert_eq!(s.pastilles[1], "Cloud : OpenAI (≈ 40 Ko)");
        assert_eq!(s.pastilles[2], "+ définitif");
    }

    #[test]
    fn test_why_strings_three_levels_and_confidence() {
        let s = WhyStrings::from_parts(
            "J'ai répondu car tu l'as demandé.",
            &[(
                "Bob habite Paris".into(),
                "fait".into(),
                "2026-10-01".into(),
                "User".into(),
            )],
            "1. lire 2. répondre",
            &["Camera".to_string()],
            0.92,
            &["ne rien faire".to_string()],
            "local",
            "0 €",
            "Local",
            "une confiance < 0,5 m'aurait fait demander",
        );
        assert_eq!(
            s.facts,
            vec!["Bob habite Paris — fait, 2026-10-01, User".to_string()]
        );
        assert_eq!(s.confidence, "haute (0.92)");
        assert_eq!(s.powers, "Camera");
        assert_eq!(s.alternatives, vec!["ne rien faire".to_string()]);
        assert_eq!(WhyStrings::confidence_word(0.2), "basse");
        assert_eq!(WhyStrings::confidence_word(0.7), "moyenne");
    }

    #[test]
    fn test_why_strings_missing_says_so() {
        let s = WhyStrings::missing("entrée sans trace");
        assert!(s.sentence.contains("pas gardé"));
        assert!(s.facts.is_empty());
        assert_eq!(s.confidence, "basse (0.00)");
        assert_eq!(s.model_line, "inconnu");
    }
}
