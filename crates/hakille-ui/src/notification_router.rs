//! Routeur de notifications cognitif et contextuel (SMNF).
//!
//! Intègre le `ChannelSelector`, le `SilenceBudgetTracker` et les inférences
//! cognitives (Regard/Gaze 3D, Fatigue PERCLOS P80, Voix) pour déterminer le
//! canal de restitution optimal et éviter la surcharge cognitive de l'utilisateur.

use crate::channel_selector::{
    ChannelDecision, ChannelSelector, DeviceType, HapticPattern, InteractionContext, OutputChannel,
    UrgencyLevel,
};
use crate::silence_budget::{InitiativeOrigin, SilenceBudgetTracker};
use hakille_substrate::{NotificationCategory, NotificationRequest, NotificationUrgency};
use serde::{Deserialize, Serialize};

/// Style visuel de présentation dans l'interface éphémère.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationVisualStyle {
    /// Carte d'action Braise `#FF7A2F` exigeant une décision humaine explicite.
    BraiseActionRequired,
    /// Toast minimaliste éphémère avec auto-évaporation fluide (1.5s - 4s).
    MinimalistToast,
    /// Impulsion subtile sur l'indicateur ambiant (point système) sans texte intrusif.
    AmbientGlyph,
    /// Synchronisation silencieuse d'arrière-plan (mobile / wearable).
    SilentShadow,
}

/// Décision d'arbitrage d'une notification par le routeur cognitif.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRoutingDecision {
    /// Canal de sortie sélectionné.
    pub channel: OutputChannel,
    /// Style visuel recommandé pour le rendu UI.
    pub visual_style: NotificationVisualStyle,
    /// Justification ergonomique et constitutionnelle de la décision.
    pub rationale: String,
    /// Motif de vibration haptique éventuel.
    pub haptic_feedback: Option<HapticPattern>,
    /// Indique si la notification a été mise en file d'attente différée (*Cognitive Deferral*).
    pub deferred: bool,
    /// Coût en crédit d'initiative débité (0 ou 1).
    pub credit_cost: u32,
}

/// État de perception cognitive instantanée transmis par M4.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveState {
    /// L'utilisateur regarde-t-il activement l'écran (Gaze Raycast 3D dans les bornes) ?
    pub user_looking_at_screen: bool,
    /// Score PERCLOS P80 de somnolence (0.0 = alerte, >0.15 = somnolence avérée).
    pub perclos_drowsiness_score: f32,
    /// Niveau de bruit ambiant mesuré par le microphone (dB).
    pub ambient_noise_db: f32,
    /// Réunion ou appel en cours détecté.
    pub in_meeting: bool,
    /// Tiers présents dans la pièce (préservation de la vie privée).
    pub third_parties_present: bool,
    /// Port d'un casque audio personnel.
    pub user_wearing_headphones: bool,
    /// Heure nocturne (> 22h00).
    pub is_night_time: bool,
}

impl Default for CognitiveState {
    fn default() -> Self {
        Self {
            user_looking_at_screen: true,
            perclos_drowsiness_score: 0.04,
            ambient_noise_db: 38.0,
            in_meeting: false,
            third_parties_present: false,
            user_wearing_headphones: false,
            is_night_time: false,
        }
    }
}

/// Routeur de notifications cognitif sovereign.
pub struct NotificationRouter {
    silence_budget: SilenceBudgetTracker,
    deferral_queue: Vec<NotificationRequest>,
}

impl Default for NotificationRouter {
    fn default() -> Self {
        Self::new(3) // 3 initiatives autonomes par jour (spec §18/§29)
    }
}

impl NotificationRouter {
    pub fn new(daily_quota: u32) -> Self {
        Self {
            silence_budget: SilenceBudgetTracker::new(daily_quota),
            deferral_queue: Vec::new(),
        }
    }

    pub fn silence_budget(&self) -> &SilenceBudgetTracker {
        &self.silence_budget
    }

    pub fn deferral_queue(&self) -> &[NotificationRequest] {
        &self.deferral_queue
    }

    pub fn clear_deferral_queue(&mut self) -> Vec<NotificationRequest> {
        std::mem::take(&mut self.deferral_queue)
    }

    /// Arbitre la livraison d'une notification en croisant le budget de silence et la perception multimodale.
    pub fn route(
        &mut self,
        request: &NotificationRequest,
        cognitive: &CognitiveState,
        device_type: DeviceType,
        origin: InitiativeOrigin,
        confidence: f64,
    ) -> NotificationRoutingDecision {
        // 1. Règle Prioritaire : Alerte de Fatigue PERCLOS > 15%
        // Une alerte de somnolence avérée outrepasse le silence habituel pour protéger l'utilisateur.
        let is_fatigue_alert = request.category == NotificationCategory::Cognitive
            && cognitive.perclos_drowsiness_score >= 0.15;

        // 2. Vérification du Budget de Silence (Spec §18/§29)
        // Les alertes critiques et rappels programmés par l'utilisateur coûtent 0 crédit.
        let is_exempt_from_budget = request.urgency >= NotificationUrgency::High
            || origin == InitiativeOrigin::UserScheduled
            || is_fatigue_alert;

        if !is_exempt_from_budget {
            let allowed = self.silence_budget.request_initiative(
                &request.id,
                origin,
                confidence,
                &request.title,
            );

            if !allowed {
                // Quota épuisé ou confiance insuffisante (<0.85) : mise en rétention différée
                self.deferral_queue.push(request.clone());
                return NotificationRoutingDecision {
                    channel: OutputChannel::Defer,
                    visual_style: NotificationVisualStyle::SilentShadow,
                    rationale: "Budget de silence épuisé (3/jour) ou confiance < 0.85 — notification différée jusqu'à transition naturelle".to_string(),
                    haptic_feedback: None,
                    deferred: true,
                    credit_cost: 0,
                };
            }
        }

        // 3. Conversion du niveau d'urgence vers le ChannelSelector
        let mut urgency_level = match request.urgency {
            NotificationUrgency::Low => UrgencyLevel::Low,
            NotificationUrgency::Normal => UrgencyLevel::Normal,
            NotificationUrgency::High => UrgencyLevel::High,
            NotificationUrgency::Critical => UrgencyLevel::Critical,
        };

        if is_fatigue_alert && urgency_level < UrgencyLevel::High {
            urgency_level = UrgencyLevel::High;
        }

        // 4. Construction du contexte d'interaction pour le ChannelSelector
        let ctx = InteractionContext {
            ambient_noise_db: cognitive.ambient_noise_db,
            third_parties_present: cognitive.third_parties_present,
            in_meeting: cognitive.in_meeting,
            user_attentive_to_screen: cognitive.user_looking_at_screen,
            user_wearing_headphones: cognitive.user_wearing_headphones,
            device_type,
            urgency: urgency_level,
            is_night_time: cognitive.is_night_time,
        };

        // 5. Arbitrage de canal
        let decision: ChannelDecision = ChannelSelector::arbitrate(&ctx);

        // 6. Détermination du style visuel
        let visual_style = if request.urgency == NotificationUrgency::Critical
            || request.actions.iter().any(|a| a.is_braise)
        {
            NotificationVisualStyle::BraiseActionRequired
        } else if cognitive.user_looking_at_screen {
            if request.urgency == NotificationUrgency::Low {
                NotificationVisualStyle::AmbientGlyph
            } else {
                NotificationVisualStyle::MinimalistToast
            }
        } else {
            NotificationVisualStyle::MinimalistToast
        };

        let credit_cost = if origin == InitiativeOrigin::SystemInferred && !is_exempt_from_budget {
            1
        } else {
            0
        };

        NotificationRoutingDecision {
            channel: decision.channel,
            visual_style,
            rationale: decision.rationale,
            haptic_feedback: decision.haptic_feedback,
            deferred: decision.channel == OutputChannel::Defer,
            credit_cost,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_silence_budget_exhaustion_defers_autonomous_suggestions() {
        let mut router = NotificationRouter::new(2); // Quota de 2
        let cognitive = CognitiveState::default();

        let req1 = NotificationRequest::new("1", "Suggestion 1", "Corps");
        let d1 = router.route(
            &req1,
            &cognitive,
            DeviceType::Desktop,
            InitiativeOrigin::SystemInferred,
            0.90,
        );
        assert!(!d1.deferred);
        assert_eq!(d1.credit_cost, 1);

        let req2 = NotificationRequest::new("2", "Suggestion 2", "Corps");
        let d2 = router.route(
            &req2,
            &cognitive,
            DeviceType::Desktop,
            InitiativeOrigin::SystemInferred,
            0.90,
        );
        assert!(!d2.deferred);
        assert_eq!(d2.credit_cost, 1);

        // 3ème suggestion : quota épuisé -> DEFERRAL
        let req3 = NotificationRequest::new("3", "Suggestion 3", "Corps");
        let d3 = router.route(
            &req3,
            &cognitive,
            DeviceType::Desktop,
            InitiativeOrigin::SystemInferred,
            0.90,
        );
        assert!(d3.deferred);
        assert_eq!(d3.channel, OutputChannel::Defer);
        assert_eq!(router.deferral_queue().len(), 1);
    }

    #[test]
    fn test_fatigue_alert_bypasses_budget_and_promotes_urgency() {
        let mut router = NotificationRouter::new(0); // 0 crédits disponibles
        let mut cognitive = CognitiveState::default();
        cognitive.perclos_drowsiness_score = 0.22; // Somnolence avérée > 0.15

        let fatigue_req = NotificationRequest::new("fatigue", "Pause Requise", "Vigilance faible")
            .with_category(NotificationCategory::Cognitive)
            .with_urgency(NotificationUrgency::Normal);

        let decision = router.route(
            &fatigue_req,
            &cognitive,
            DeviceType::Desktop,
            InitiativeOrigin::SystemInferred,
            0.95,
        );

        assert!(
            !decision.deferred,
            "Fatigue alert must not be suppressed by silence budget"
        );
        assert_eq!(decision.channel, OutputChannel::EphemeralHud);
    }

    #[test]
    fn test_meeting_and_third_parties_prevents_voice_leak() {
        let mut router = NotificationRouter::new(3);
        let mut cognitive = CognitiveState::default();
        cognitive.in_meeting = true;
        cognitive.third_parties_present = true;

        let critical_req = NotificationRequest::new("crit", "Alerte Sécurité", "Détail")
            .with_urgency(NotificationUrgency::Critical);

        let decision = router.route(
            &critical_req,
            &cognitive,
            DeviceType::Desktop,
            InitiativeOrigin::UserScheduled,
            1.0,
        );

        // En réunion avec tiers, même une alerte critique utilise le HUD pour éviter la fuite audio
        assert_eq!(decision.channel, OutputChannel::EphemeralHud);
        assert_eq!(
            decision.visual_style,
            NotificationVisualStyle::BraiseActionRequired
        );
    }

    #[test]
    fn test_gaze_aware_micro_pulse_for_low_urgency_focused_user() {
        let mut router = NotificationRouter::new(3);
        let mut cognitive = CognitiveState::default();
        cognitive.user_looking_at_screen = true;

        let low_req = NotificationRequest::new("info", "Sync terminée", "Triplets synchronisés")
            .with_urgency(NotificationUrgency::Low);

        let decision = router.route(
            &low_req,
            &cognitive,
            DeviceType::Desktop,
            InitiativeOrigin::UserScheduled,
            1.0,
        );

        assert_eq!(decision.visual_style, NotificationVisualStyle::AmbientGlyph);
    }
}
