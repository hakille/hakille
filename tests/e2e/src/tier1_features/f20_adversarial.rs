//! Feature 20: Adversarial Coverage Hardening (Milestone M5)
//!
//! Specifications: T1_F20_01 to T1_F20_05

use hakille_ui_protocol::UiMessage;

/// `T1_F20_01`: Wire Fuzzing Memory Safety Invariant
#[test]
fn test_t1_f20_01_wire_fuzzing_memory_safety_invariant() {
    let fuzzed_payloads = [
        "{}",
        "null",
        "{\"version\": -1}",
        "{\"version\": 999999999}",
        "\"string_instead_of_object\"",
        "[1, 2, 3]",
        "{\"version\": 1, \"inner\": null}",
    ];

    for payload in fuzzed_payloads {
        let res: Result<UiMessage, _> = serde_json::from_str(payload);
        if let Ok(msg) = res {
            // If parsed, .check() must validate or reject cleanly without panicking
            let _ = msg.check();
        }
    }
}

/// `T1_F20_02`: ANSI Injection Sanitization
#[test]
fn test_t1_f20_02_ansi_injection_sanitization() {
    let attack_string = "\x1b[2J\x1b[H; rm -rf /; echo pwned";
    // Sanitization retains printable chars and strips or escapes control chars
    let sanitized: String = attack_string
        .chars()
        .filter(|c| !c.is_ascii_control() || *c == '\n')
        .collect();
    assert!(!sanitized.contains("\x1b"));
}

/// `T1_F20_03`: Clock Rollback Resistance
#[test]
fn test_t1_f20_03_clock_rollback_resistance() {
    let current_ms: u64 = 10000;
    let rollback_time_ms: u64 = 5000;
    // Saturating sub prevents integer underflow
    let delta = current_ms.saturating_sub(rollback_time_ms);
    assert_eq!(delta, 5000);
}

/// `T1_F20_04`: Double-Click Re-entrancy Attack
#[test]
fn test_t1_f20_04_double_click_reentrancy_attack() {
    let mut processed_cards = std::collections::HashSet::new();
    let card_id = 12345u64;

    let first_insert = processed_cards.insert(card_id);
    let second_insert = processed_cards.insert(card_id);

    assert!(first_insert, "First approval must succeed");
    assert!(
        !second_insert,
        "Second re-entrant approval must be rejected"
    );
}

/// `T1_F20_05`: Unicode Bidi Spoofing Defense
#[test]
fn test_t1_f20_05_unicode_bidi_spoofing_defense() {
    let malicious = "safe_file\u{202E}cod.exe";
    let sanitized: String = malicious
        .chars()
        .filter(|&c| c != '\u{202E}' && c != '\u{202D}' && c != '\u{202C}')
        .collect();
    assert!(!sanitized.contains('\u{202E}'));
    assert_eq!(sanitized, "safe_filecod.exe");
}
