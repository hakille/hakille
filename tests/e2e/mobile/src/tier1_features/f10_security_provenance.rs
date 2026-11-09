//! Feature 10: Security Provenance Banner (Milestone M1 / Spec §17)
//!
//! Specifications: T1_F10_01 to T1_F10_05
//!
//! Verifies:
//! - ProvenanceBand variant labeling (Hakille, VerifiedSkill, External).
//! - Trusted rendering text sanitization: strips skill markup, HTML tags, and ANSI escapes.
//! - External block specs: 2 px side rule and mandatory 'externe' badge.
//! - Skill squatting and homograph impersonation detection.
//! - Sensitive clipboard expiry TTL (60 seconds).

use hakille_design::provenance::{
    CLIPBOARD_SENSITIVE_TTL_SECS, ExternalBlock, ProvenanceBand, sanitize_skill_text,
};

/// `T1_F10_01`: Provenance band labeling.
///
/// Ensures clear, unambiguous provenance labels for native Hakille skills,
/// third-party verified skills, and untrusted external content.
#[test]
fn test_t1_f10_01_provenance_band_labels() {
    let hakille = ProvenanceBand::Hakille;
    assert_eq!(hakille.label(), "Hakille");

    let verified = ProvenanceBand::VerifiedSkill {
        publisher: "MétéoFrance".into(),
        name: "previsions".into(),
    };
    assert_eq!(verified.label(), "skill vérifié (MétéoFrance · previsions)");

    let external = ProvenanceBand::External;
    assert_eq!(external.label(), "externe");
}

/// `T1_F10_02`: Trusted rendering markup sanitization.
///
/// Skills cannot inject HTML, script tags, or terminal ANSI escape sequences
/// into rendered ActionCards. All markup is stripped while preserving payload text.
#[test]
fn test_t1_f10_02_skill_markup_sanitization() {
    let hostile_input = "<script src=\"evil.js\"></script><iframe src=\"malicious.site\"></iframe>\
        <b style=\"color:red\">Virement urgent</b>\u{001b}[31m\u{001b}[1m";

    let sanitized = sanitize_skill_text(hostile_input);

    assert!(
        !sanitized.contains('<') && !sanitized.contains('>'),
        "Must strip all HTML tags: {sanitized}"
    );
    assert!(
        !sanitized.contains("\u{001b}"),
        "Must strip ANSI escape sequences: {sanitized}"
    );
    assert!(
        sanitized.contains("Virement urgent"),
        "Must preserve inner plain text: {sanitized}"
    );
}

/// `T1_F10_03`: External content block specifications.
///
/// Untrusted external text must render with a 2 px boundary rule and an explicit
/// 'externe' badge, preventing it from masquerading as system instructions.
#[test]
fn test_t1_f10_03_external_content_block_specs() {
    assert_eq!(
        ExternalBlock::side_rule_px(),
        2,
        "External content must have a 2 px side rule"
    );
    assert_eq!(
        ExternalBlock::badge(),
        "externe",
        "External block badge must be 'externe'"
    );

    let block = ExternalBlock {
        text: "Corps d'un courriel entrant non vérifié".into(),
    };
    assert_eq!(block.text, "Corps d'un courriel entrant non vérifié");
}

/// `T1_F10_04`: Skill squatting and homograph impersonation detection.
///
/// Detects names designed to impersonate installed skills (e.g. 'mail-plus' or
/// 'SuperMail' when 'mail' is already installed).
#[test]
fn test_t1_f10_04_skill_squatting_homograph_detection() {
    let installed = vec!["mail".to_string(), "calendar".to_string()];

    // Impersonating skill (contains 'mail' but is not 'mail')
    let squatter = ProvenanceBand::VerifiedSkill {
        publisher: "attacker".into(),
        name: "mail-plus".into(),
    };
    assert!(
        squatter.looks_like_installed(&installed),
        "Must flag 'mail-plus' as squatter against 'mail'"
    );

    // Exact installed skill is legitimate (not a squatter)
    let legitimate = ProvenanceBand::VerifiedSkill {
        publisher: "Hakille".into(),
        name: "mail".into(),
    };
    assert!(
        !legitimate.looks_like_installed(&installed),
        "Legitimate installed skill must not flag itself"
    );

    // Unrelated skill is legitimate
    let unrelated = ProvenanceBand::VerifiedSkill {
        publisher: "MusicCorp".into(),
        name: "synthesizer".into(),
    };
    assert!(!unrelated.looks_like_installed(&installed));
}

/// `T1_F10_05`: Sensitive clipboard TTL constant.
///
/// Sensitive data copied to the clipboard expires after exactly 60 seconds
/// to mitigate cross-app clipboard exfiltration.
#[test]
fn test_t1_f10_05_sensitive_clipboard_ttl_constant() {
    assert_eq!(
        CLIPBOARD_SENSITIVE_TTL_SECS, 60,
        "Clipboard sensitive data TTL must be 60 seconds"
    );
}
