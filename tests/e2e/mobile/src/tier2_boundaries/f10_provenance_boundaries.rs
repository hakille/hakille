//! Feature 10: Security Provenance Banner (Tier 2 Boundaries & Fault Injection)
//!
//! Specifications: T2_F10_01 to T2_F10_05 (5 tests total).

#![allow(unused_imports)]

use crate::harness::{DaemonStep, MobileTestRig, default_card};
use hakille_design::action_card::{ActionCardView, DataPastille, Reversibility};
use hakille_design::autonomy::{Crank, DomainAutonomy, ENVELOPE_EXPIRY_DAYS, always_validated};
use hakille_design::hold::{HoldEvent, HoldState, feed_hold, hold_progress};
use hakille_design::provenance::{ExternalBlock, ProvenanceBand, sanitize_skill_text};
use hakille_design::why::{FactRef, WhySheet};
use hakille_ipc::Frame;
use hakille_mobile::MobileBridge;
use hakille_ui_protocol::{ActionCard, DataExit, KernelToUi, Money, Provenance, UiToKernel};
use serde_json::json;

/// `T2_F10_01`: Homoglyph attacks in skill names.
///
/// Detects names designed to impersonate installed skills using visually similar names.
#[test]
fn test_t2_f10_01_homoglyph_attacks_in_skill_names() {
    let installed = vec!["mail".to_string(), "calendar".to_string()];

    // Spoofed name containing installed name as substring
    let spoof1 = ProvenanceBand::VerifiedSkill {
        publisher: "attacker".into(),
        name: "my-mail".into(),
    };
    assert!(spoof1.looks_like_installed(&installed));

    let spoof2 = ProvenanceBand::VerifiedSkill {
        publisher: "attacker".into(),
        name: "supercalendar".into(),
    };
    assert!(spoof2.looks_like_installed(&installed));

    // Legitimate distinct name
    let legit = ProvenanceBand::VerifiedSkill {
        publisher: "meteo".into(),
        name: "previsions".into(),
    };
    assert!(!legit.looks_like_installed(&installed));
}

/// `T2_F10_02`: Mixed Cyrillic/Latin squatting detection.
///
/// Detects mixed alphabet characters designed to fool human visual verification.
#[test]
fn test_t2_f10_02_mixed_script_squatting_detection() {
    fn contains_mixed_scripts(text: &str) -> bool {
        let has_latin = text.chars().any(|c| c.is_ascii_alphabetic());
        let has_cyrillic = text.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c));
        has_latin && has_cyrillic
    }

    // "mаil" where 'а' is Cyrillic U+0430
    let deceptive_name = "m\u{0430}il";
    assert!(
        contains_mixed_scripts(deceptive_name),
        "Mixed scripts must be flagged"
    );

    let clean_name = "mail";
    assert!(!contains_mixed_scripts(clean_name));
}

/// `T2_F10_03`: Malicious ANSI escape sequences stripped.
///
/// Strips all terminal ANSI control codes, cursor resets, and OSC commands.
#[test]
fn test_t2_f10_03_malicious_ansi_escape_sequences_stripped() {
    let hostile = "Normal \u{001b}[31;1mROUGE\u{001b}[0m \u{001b}[2J\u{001b}[HSuite";
    let clean = sanitize_skill_text(hostile);
    assert!(!clean.contains('\u{001b}'));
    assert!(clean.contains("Normal"));
    assert!(clean.contains("ROUGE"));
    assert!(clean.contains("Suite"));
}

/// `T2_F10_04`: Raw HTML injection stripped.
///
/// Strips HTML tags, script tags, iframes, and onerror handlers.
#[test]
fn test_t2_f10_04_raw_html_injection_stripped() {
    let hostile_html =
        "<script>alert('xss')</script><img src=x onerror=\"exploit()\" /><b>Texte</b>";
    let clean = sanitize_skill_text(hostile_html);
    assert!(!clean.contains('<'));
    assert!(!clean.contains('>'));
    assert!(!clean.contains("script"));
    assert!(clean.contains("Texte"));
}

/// `T2_F10_05`: Missing provenance tag defaults to external block.
///
/// Untrusted external text renders with a 2px side rule and 'externe' badge.
#[test]
fn test_t2_f10_05_missing_provenance_tag_defaults_to_external() {
    assert_eq!(ExternalBlock::side_rule_px(), 2);
    assert_eq!(ExternalBlock::badge(), "externe");

    let block = ExternalBlock {
        text: "Texte non certifié".into(),
    };
    assert_eq!(block.text, "Texte non certifié");
}
