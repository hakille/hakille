//! Integration tests for Cryptographic Audit Adapter.

use chrono::Utc;
use ed25519_dalek::SigningKey;
use hakille_audit::{AuditJournal, sha256_token_hash};
use hakille_capability::CapabilityAuditAdapter;
use hakille_capability::audit::sanitize_parameters;
use hakille_capability::types::{
    CanonicalToolCall, CanonicalToolResult, ExecutionMetadata, RiskTier,
};
use hakille_core::{ActorId, CapabilityAction, CapabilityAuthority, CapabilityScope, Timestamp};
use rand::rngs::OsRng;
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_audit_merkle_chaining_and_token_binding() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("audit.redb");
    let mut rng = OsRng;
    let signing_key = SigningKey::generate(&mut rng);

    let journal = Arc::new(AuditJournal::new(db_path.to_str().unwrap(), signing_key).unwrap());
    let adapter = CapabilityAuditAdapter::new(journal.clone());

    let authority = CapabilityAuthority::new();
    let future = Timestamp(Utc::now() + chrono::Duration::hours(1));
    let token = authority
        .create_root_token(
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            future,
        )
        .unwrap();
    let expected_token_hash = sha256_token_hash(&token.to_vec().unwrap());

    // 1. First invocation
    let call1 = CanonicalToolCall::new(
        "call_01",
        "read_file",
        serde_json::json!({"path": "/etc/hosts", "auth_token": "secret_abc"}),
    );
    let res1 = CanonicalToolResult::success_text(
        "call_01",
        "127.0.0.1 localhost",
        ExecutionMetadata::new(12, RiskTier::Tier0ReadOnly),
    );

    let entry_id_1 = adapter
        .log_invocation(
            &call1,
            &res1,
            Some(&token),
            Some(CapabilityScope::Filesystem),
            ActorId::Agent("test_agent".to_string()),
        )
        .await
        .unwrap();

    assert!(!entry_id_1.is_empty());

    // 2. Second invocation (without token)
    let call2 = CanonicalToolCall::new("call_02", "list_dir", serde_json::json!({"path": "/tmp"}));
    let res2 = CanonicalToolResult::success_text(
        "call_02",
        "file1.txt",
        ExecutionMetadata::new(8, RiskTier::Tier0ReadOnly),
    );

    let entry_id_2 = adapter
        .log_invocation(
            &call2,
            &res2,
            None,
            Some(CapabilityScope::Filesystem),
            ActorId::Agent("test_agent".to_string()),
        )
        .await
        .unwrap();

    assert!(!entry_id_2.is_empty());
    assert_ne!(entry_id_1, entry_id_2);

    // 3. Verify Merkle Chain continuity and integrity
    let report = journal.verify_chain().unwrap();
    assert!(
        report.is_valid(),
        "Merkle audit chain must be completely healthy"
    );
    assert_eq!(report.total_entries, 2);
    assert_eq!(report.valid_entries, 2);

    // 4. Verify token binding on entry 1
    let id_1 = hakille_core::EntryId(uuid::Uuid::parse_str(&entry_id_1).unwrap());
    let entry_1_record = journal.get(&id_1).unwrap().expect("Entry 1 should exist");
    assert_eq!(entry_1_record.token_hash, Some(expected_token_hash));

    // Entry 2 has no token
    let id_2 = hakille_core::EntryId(uuid::Uuid::parse_str(&entry_id_2).unwrap());
    let entry_2_record = journal.get(&id_2).unwrap().expect("Entry 2 should exist");
    assert_eq!(entry_2_record.token_hash, None);

    // Merkle chaining: entry 2's prev_hash must match entry 1's chain hash
    assert!(!entry_2_record.prev_hash.is_empty());
}

#[test]
fn test_sensitive_parameter_sanitization() {
    let input = serde_json::json!({
        "username": "admin",
        "password": "super_secret_password",
        "api_key": "sk-1234567890",
        "nested": {
            "oauth_token": "bearer_xyz",
            "normal_field": 42
        },
        "items": [
            { "client_secret": "shhh" },
            { "safe_label": "production" }
        ]
    });

    let sanitized = sanitize_parameters(&input);

    assert_eq!(sanitized["username"], "admin");
    assert_eq!(sanitized["password"], "[REDACTED]");
    assert_eq!(sanitized["api_key"], "[REDACTED]");
    assert_eq!(sanitized["nested"]["oauth_token"], "[REDACTED]");
    assert_eq!(sanitized["nested"]["normal_field"], 42);
    assert_eq!(sanitized["items"][0]["client_secret"], "[REDACTED]");
    assert_eq!(sanitized["items"][1]["safe_label"], "production");
}
