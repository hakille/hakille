//! Modèles de données et interfaces pour le sous-système de notifications souverain (SMNF).
//!
//! Couvre les besoins multi-plateformes : Linux (D-Bus), macOS (UserNotifications),
//! Windows (Toast/Tray), Mobile (Shadow Sync), Web (Notification API) et Redox (Scheme).

use serde::{Deserialize, Serialize};

/// Niveau d'urgence d'une notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NotificationUrgency {
    /// Information d'arrière-plan, sans interruption sonore ni vibration.
    Low,
    /// Notification nominale de l'OS ou d'un agent.
    Normal,
    /// Alerte requérant l'attention rapide de l'utilisateur (alerte fatigue, calendrier immédiat).
    High,
    /// Alarme critique ou demande de décision constitutionnelle / sécurité (Braise #FF7A2F).
    Critical,
}

impl Default for NotificationUrgency {
    fn default() -> Self {
        Self::Normal
    }
}

/// Catégorie fonctionnelle de la notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NotificationCategory {
    /// Événements du noyau ou de l'OS (mises à jour, batterie, périphériques).
    System,
    /// Inférences cognitives (vigilance, fatigue PERCLOS, posture, rappels bien-être).
    Cognitive,
    /// Messages ou requêtes émis par les agents WASM ou compétences vérifiées.
    Agent,
    /// Alertes de souveraineté matérielle, audits, et requêtes de signature cryptographique.
    Security,
    /// Opérations et budgets financiers locaux.
    Wallet,
}

impl Default for NotificationCategory {
    fn default() -> Self {
        Self::System
    }
}

/// Action interactive rattachée à une notification (bouton de réponse).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationAction {
    /// Identifiant unique de l'action pour le rappel callback (ex: "approve", "snooze", "dismiss").
    pub id: String,
    /// Libellé lisible affiché sur le bouton (ex: "Valider", "Reporter 10m").
    pub label: String,
    /// Action irréversible ou comportant un impact matériel/financier.
    pub is_destructive: bool,
    /// Exige la coloration d'accentuation Braise `#FF7A2F` (strictement réservée aux confirmations).
    pub is_braise: bool,
}

impl NotificationAction {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            is_destructive: false,
            is_braise: false,
        }
    }

    pub fn braise(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            is_destructive: true,
            is_braise: true,
        }
    }
}

/// Requête unifiée de notification multi-plateforme.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationRequest {
    /// Identifiant unique (UUID ou déterministe).
    pub id: String,
    /// Titre concis de la notification.
    pub title: String,
    /// Corps du message (texte brut ou markdown léger).
    pub body: String,
    /// Niveau d'urgence.
    pub urgency: NotificationUrgency,
    /// Catégorie thématique.
    pub category: NotificationCategory,
    /// Actions interactives proposées à l'utilisateur.
    pub actions: Vec<NotificationAction>,
    /// Délai d'auto-évaporation en millisecondes (`None` = persistant jusqu'à interaction).
    pub timeout_ms: Option<u32>,
    /// Profil sonore optionnel ("chime", "whisper", "silent").
    pub sound: Option<String>,
    /// Profil haptique optionnel ("pulse", "double_tap", "urgent_buzz").
    pub haptic: Option<String>,
    /// Tag de déduplication : remplace toute notification active portant le même tag.
    pub dedup_tag: Option<String>,
    /// Horodatage de création en millisecondes Unix.
    pub created_at_ms: u64,
}

impl NotificationRequest {
    pub fn new(id: impl Into<String>, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            body: body.into(),
            urgency: NotificationUrgency::Normal,
            category: NotificationCategory::System,
            actions: Vec::new(),
            timeout_ms: Some(4000),
            sound: Some("chime".to_string()),
            haptic: None,
            dedup_tag: None,
            created_at_ms: chrono::Utc::now().timestamp_millis() as u64,
        }
    }

    pub fn with_urgency(mut self, urgency: NotificationUrgency) -> Self {
        self.urgency = urgency;
        if urgency == NotificationUrgency::Critical {
            self.timeout_ms = None; // Les alertes critiques persistent
            self.haptic = Some("urgent_buzz".to_string());
        }
        self
    }

    pub fn with_category(mut self, category: NotificationCategory) -> Self {
        self.category = category;
        self
    }

    pub fn with_action(mut self, action: NotificationAction) -> Self {
        self.actions.push(action);
        self
    }

    pub fn with_dedup_tag(mut self, tag: impl Into<String>) -> Self {
        self.dedup_tag = Some(tag.into());
        self
    }

    pub fn with_timeout_ms(mut self, timeout_ms: Option<u32>) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }
}

/// Ticket d'acquittement d'une notification émise par le substrate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NotificationHandle {
    /// Identifiant de la notification.
    pub id: String,
    /// Canal physique de restitution utilisé ("dbus", "apple_script", "win32_tray", "shadow_sync", "web_notification", "redox_scheme").
    pub channel: String,
    /// Horodatage Unix de prise en charge en millisecondes.
    pub delivered_at_ms: u64,
}

/// Résultat ou retour d'interaction sur une notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationResponse {
    /// La notification a été fermée ou ignorée.
    Dismissed { id: String, reason: String },
    /// L'utilisateur a cliqué sur une action interactive spécifique.
    ActionClicked { id: String, action_id: String },
    /// La notification a expiré selon son délai d'auto-évaporation.
    Expired { id: String },
}
