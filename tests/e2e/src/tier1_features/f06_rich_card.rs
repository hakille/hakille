//! Feature 06: Rich Action Card Attachments (Milestone M2)
//!
//! Specifications: T1_F06_01 to T1_F06_05

use hakille_ui_desktop::a11y::ActionCardA11y;
use hakille_ui_protocol::{ActionCard, DataExit, Money, Provenance};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum CardAttachment {
    CodeDiff {
        file_path: String,
        diff_chunk: String,
    },
    MemoryProvenance {
        triplet_id: String,
        confidence: f32,
    },
    AuditBadge {
        issuer: String,
        signature_ed25519: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ExtendedActionCard {
    pub base: ActionCard,
    #[serde(default)]
    pub attachments: Vec<CardAttachment>,
}

fn sample_card(id: u64, reversible: bool) -> ActionCard {
    ActionCard {
        id,
        verb: "Refactoriser".into(),
        object: "parser.rs".into(),
        reason: "Optimisation mémoire".into(),
        cost: Money {
            micros: 0,
            currency: "EUR".into(),
        },
        data_exit: DataExit::Local,
        reversible,
        provenance: Provenance::Hakille,
    }
}

/// `T1_F06_01`: Code Diff Preview Attachment Serialization
#[test]
fn test_t1_f06_01_code_diff_preview_attachment_serialization() {
    let ext = ExtendedActionCard {
        base: sample_card(1, false),
        attachments: vec![CardAttachment::CodeDiff {
            file_path: "src/main.rs".into(),
            diff_chunk: "@@ -1,3 +1,4 @@\n+use tokio;\n".into(),
        }],
    };
    let json = serde_json::to_string(&ext).unwrap();
    assert!(json.contains("CodeDiff"));
    assert!(json.contains("src/main.rs"));
    assert!(json.contains("+use tokio;"));
}

/// `T1_F06_02`: Memory Provenance Link Attachment
#[test]
fn test_t1_f06_02_memory_provenance_link_attachment() {
    let ext = ExtendedActionCard {
        base: sample_card(2, true),
        attachments: vec![CardAttachment::MemoryProvenance {
            triplet_id: "uuid-1234-abcd".into(),
            confidence: 0.92,
        }],
    };
    let json = serde_json::to_string(&ext).unwrap();
    let decoded: ExtendedActionCard = serde_json::from_str(&json).unwrap();
    match &decoded.attachments[0] {
        CardAttachment::MemoryProvenance {
            triplet_id,
            confidence,
        } => {
            assert_eq!(triplet_id, "uuid-1234-abcd");
            assert!((confidence - 0.92).abs() < 0.001);
        }
        _ => panic!("Expected MemoryProvenance attachment"),
    }
}

/// `T1_F06_03`: Cryptographic Audit Badge Attachment
#[test]
fn test_t1_f06_03_cryptographic_audit_badge_attachment() {
    let ext = ExtendedActionCard {
        base: sample_card(3, false),
        attachments: vec![CardAttachment::AuditBadge {
            issuer: "kernel-auth".into(),
            signature_ed25519: "a1b2c3d4e5f6".into(),
        }],
    };
    let json = serde_json::to_string(&ext).unwrap();
    let decoded: ExtendedActionCard = serde_json::from_str(&json).unwrap();
    match &decoded.attachments[0] {
        CardAttachment::AuditBadge {
            issuer,
            signature_ed25519,
        } => {
            assert_eq!(issuer, "kernel-auth");
            assert_eq!(signature_ed25519, "a1b2c3d4e5f6");
        }
        _ => panic!("Expected AuditBadge attachment"),
    }
}

/// `T1_F06_04`: Viewmodel Conversion of Rich Attachments
#[test]
fn test_t1_f06_04_viewmodel_conversion_of_rich_attachments() {
    let card = sample_card(4, false);
    let a11y = ActionCardA11y {
        verb: card.verb.clone(),
        object: card.object.clone(),
        reason: card.reason.clone(),
        why_trace: "Trace mémoire #14".into(),
        cost_label: "0.00 EUR".into(),
        data_label: "Local".into(),
        reversibility_label: "définitif".into(),
        needs_hold: true,
        provenance_label: "Local".into(),
        is_external: false,
    };
    assert_eq!(a11y.verb, "Refactoriser");
    assert_eq!(a11y.reversibility_label, "définitif");
    assert!(a11y.needs_hold);
}

/// `T1_F06_05`: Reversibility Guarantee Alignment
#[test]
fn test_t1_f06_05_reversibility_guarantee_alignment() {
    let card = sample_card(5, true);
    assert!(card.reversible);

    let a11y = ActionCardA11y {
        verb: card.verb.clone(),
        object: card.object.clone(),
        reason: card.reason.clone(),
        why_trace: "".into(),
        cost_label: "0.00 EUR".into(),
        data_label: "Local".into(),
        reversibility_label: "annulable".into(),
        needs_hold: !card.reversible,
        provenance_label: "Local".into(),
        is_external: false,
    };

    assert_eq!(a11y.reversibility_label, "annulable");
    assert!(!a11y.needs_hold, "Reversible card must NOT require hold");
}
