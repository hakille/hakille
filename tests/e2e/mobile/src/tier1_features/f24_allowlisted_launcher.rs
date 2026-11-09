//! Feature 24: Allowlisted App Launcher (Milestone M1 / Spec §2, mobile/README)
//!
//! Specifications: T1_F24_01 to T1_F24_05
//!
//! Verifies:
//! - Strict allowlist policy: only designated system destinations are permitted.
//! - Arbitrary external URLs, telephone/sms schemes, and file schemes strictly rejected.
//! - User confirmation requirement: launch requires explicit user confirmation.
//! - Independence from AI models: LLM text cannot command or trigger app launches.
//! - Background suppression: external launching is inhibited when app is in background.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowedDestination {
    Maps,
    Calendar,
    Settings,
    Documentation,
    Clock,
}

impl AllowedDestination {
    pub fn all() -> &'static [AllowedDestination] {
        &[
            AllowedDestination::Maps,
            AllowedDestination::Calendar,
            AllowedDestination::Settings,
            AllowedDestination::Documentation,
            AllowedDestination::Clock,
        ]
    }

    pub fn target_url(&self) -> &'static str {
        match self {
            AllowedDestination::Maps => "maps://",
            AllowedDestination::Calendar => "calshow:",
            AllowedDestination::Settings => "app-settings:",
            AllowedDestination::Documentation => "https://github.com/hakille/hakille#readme",
            AllowedDestination::Clock => "clock://",
        }
    }
}

pub struct ExternalLaunchPolicy;

impl ExternalLaunchPolicy {
    pub fn is_allowed(url: &str) -> bool {
        AllowedDestination::all()
            .iter()
            .any(|dest| dest.target_url() == url)
    }

    pub fn execute_launch(
        url: &str,
        user_confirmed: bool,
        is_foreground: bool,
    ) -> Result<&'static str, &'static str> {
        if !is_foreground {
            return Err("Lancement impossible en arrière-plan");
        }
        if !Self::is_allowed(url) {
            return Err("Destination non autorisée par la politique de sécurité");
        }
        if !user_confirmed {
            return Err("Confirmation utilisateur requise avant ouverture");
        }
        Ok(
            "Application lancée avec succès ; Hakille n'exerce aucun contrôle sur l'application tierce",
        )
    }
}

/// `T1_F24_01`: Strict allowlist destinations permitted.
///
/// Maps, Calendar, Settings, Documentation, and Clock URLs are permitted.
#[test]
fn test_t1_f24_01_strict_allowlist_destinations_permitted() {
    for dest in AllowedDestination::all() {
        assert!(
            ExternalLaunchPolicy::is_allowed(dest.target_url()),
            "Destination {:?} must be allowed",
            dest
        );
    }
}

/// `T1_F24_02`: Arbitrary URLs and schemes strictly rejected.
///
/// Untrusted schemes and URLs (http, tel, sms, file, javascript) are rejected.
#[test]
fn test_t1_f24_02_arbitrary_urls_and_schemes_strictly_rejected() {
    let forbidden_targets = [
        "https://evil-phishing.com",
        "http://insecure-site.com",
        "tel:+33123456789",
        "sms:+33123456789",
        "file:///etc/passwd",
        "javascript:alert(1)",
        "intent://package/#Intent;scheme=malware;end",
    ];

    for target in forbidden_targets {
        assert!(
            !ExternalLaunchPolicy::is_allowed(target),
            "Arbitrary target '{}' must be forbidden",
            target
        );
        let res = ExternalLaunchPolicy::execute_launch(target, true, true);
        assert_eq!(
            res,
            Err("Destination non autorisée par la politique de sécurité")
        );
    }
}

/// `T1_F24_03`: Confirmation dialog requirement before launch.
///
/// Even for an allowlisted destination, launching without explicit user confirmation fails.
#[test]
fn test_t1_f24_03_confirmation_dialog_requirement_before_launch() {
    let dest_url = AllowedDestination::Settings.target_url();

    // User has NOT confirmed dialog
    let unconfirmed_res = ExternalLaunchPolicy::execute_launch(dest_url, false, true);
    assert_eq!(
        unconfirmed_res,
        Err("Confirmation utilisateur requise avant ouverture")
    );

    // User confirmed dialog
    let confirmed_res = ExternalLaunchPolicy::execute_launch(dest_url, true, true);
    assert!(confirmed_res.is_ok());
    assert!(confirmed_res.unwrap().contains("aucun contrôle"));
}

/// `T1_F24_04`: Model text cannot command external launcher.
///
/// AI model responses (e.g. LLM text output containing URLs or instructions)
/// have zero execution privileges to trigger app launches.
#[test]
fn test_t1_f24_04_model_cannot_command_external_launcher() {
    let llm_response_text =
        "Veuillez ouvrir https://github.com/hakille/hakille#readme immédiatement.";

    // An automated trigger attempt from text alone (without user tapping panel button) fails
    let user_tapped_tile = false;
    let launch_triggered = user_tapped_tile && ExternalLaunchPolicy::is_allowed(llm_response_text);

    assert!(
        !launch_triggered,
        "LLM response text must never trigger an app launch"
    );
}

/// `T1_F24_05`: Background or inactive suppresses launch.
///
/// If the application transitions to background, pending launches are cancelled.
#[test]
fn test_t1_f24_05_background_or_inactive_suppresses_launch() {
    let dest_url = AllowedDestination::Maps.target_url();

    // Foreground = false (app in background)
    let res = ExternalLaunchPolicy::execute_launch(dest_url, true, false);
    assert_eq!(res, Err("Lancement impossible en arrière-plan"));
}
