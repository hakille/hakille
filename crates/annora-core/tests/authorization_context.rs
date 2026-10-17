//! Batterie d'enforcement : refus token expiré, hors-scope, audience
//! mismatch, traversal `../evil`, monotonie d'atténuation (scope, action,
//! expiry) + chemin valide accepté.

use annora_core::{
    AuthorizationContext, CapabilityAction, CapabilityAuthority, CapabilityScope, Quota, Timestamp,
};

fn future_ts() -> Timestamp {
    Timestamp(chrono::Utc::now() + chrono::Duration::hours(1))
}

fn past_ts() -> Timestamp {
    Timestamp(chrono::Utc::now() - chrono::Duration::hours(1))
}

fn fs_read_ctx(expiry: Timestamp) -> AuthorizationContext {
    AuthorizationContext::new(
        "user:test",
        "annora-kernel",
        expiry,
        CapabilityScope::Filesystem,
        CapabilityAction::Read,
        "notes/todo.md",
        None,
        "nonce-int-1234abcd",
        vec![],
    )
    .expect("contexte valide")
}

#[test]
fn refuses_expired_token() {
    let authority = CapabilityAuthority::new();
    let ctx = fs_read_ctx(future_ts());
    let expired = authority
        .create_root_token(
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            past_ts(),
        )
        .unwrap();
    assert!(expired.is_expired());
    let err = ctx
        .authorize(&expired, &authority, "annora-kernel")
        .unwrap_err();
    assert!(
        err.to_string().contains("expired") || err.to_string().contains("expiré"),
        "{err}"
    );
}

#[test]
fn refuses_out_of_scope_token() {
    let authority = CapabilityAuthority::new();
    let ctx = fs_read_ctx(future_ts());
    // Token Network/Read pour une requête Filesystem/Read.
    let net = authority
        .create_root_token(
            vec![CapabilityScope::Network],
            vec![CapabilityAction::Read],
            future_ts(),
        )
        .unwrap();
    let err = ctx
        .authorize(&net, &authority, "annora-kernel")
        .unwrap_err();
    assert!(err.to_string().contains("scope insuffisant"), "{err}");
    // Mauvaise action (Write au lieu de Read).
    let write = authority
        .create_root_token(
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Write],
            future_ts(),
        )
        .unwrap();
    let err = ctx
        .authorize(&write, &authority, "annora-kernel")
        .unwrap_err();
    assert!(err.to_string().contains("scope insuffisant"), "{err}");
}

#[test]
fn refuses_audience_mismatch() {
    let authority = CapabilityAuthority::new();
    let ctx = fs_read_ctx(future_ts());
    let token = authority
        .create_root_token(
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            future_ts(),
        )
        .unwrap();
    assert!(ctx.authorize(&token, &authority, "annora-kernel").is_ok());
    let err = ctx
        .authorize(&token, &authority, "annora-daemon")
        .unwrap_err();
    assert!(err.to_string().contains("audience mismatch"), "{err}");
}

#[test]
fn refuses_traversal_and_outside_vfs() {
    for evil in [
        "../evil",
        "a/../../evil",
        "/vfs/a/../evil",
        "/etc/passwd",
        "/evil",
        "C:/evil",
    ] {
        assert!(
            AuthorizationContext::new(
                "user:test",
                "annora-kernel",
                future_ts(),
                CapabilityScope::Filesystem,
                CapabilityAction::Read,
                evil,
                None,
                "nonce-int-1234abcd",
                vec![],
            )
            .is_err(),
            "aurait dû refuser : {evil}"
        );
    }
    // Chemins VFS valides acceptés.
    for ok in ["notes/todo.md", "/vfs/notes/todo.md", "/tmp/annora-dryrun"] {
        assert!(
            AuthorizationContext::new(
                "user:test",
                "annora-kernel",
                future_ts(),
                CapabilityScope::Filesystem,
                CapabilityAction::Read,
                ok,
                None,
                "nonce-int-1234abcd",
                vec![],
            )
            .is_ok(),
            "aurait dû accepter : {ok}"
        );
    }
}

#[test]
fn attenuation_is_monotone() {
    let authority = CapabilityAuthority::new();
    let root = authority
        .create_root_token(
            vec![CapabilityScope::Filesystem, CapabilityScope::Network],
            vec![CapabilityAction::Read, CapabilityAction::Write],
            future_ts(),
        )
        .unwrap();
    // Élargissement de scope refusé.
    assert!(
        authority
            .attenuate(
                &root,
                vec![CapabilityScope::Wallet],
                vec![CapabilityAction::Read],
                Timestamp(chrono::Utc::now() + chrono::Duration::minutes(5)),
            )
            .is_err()
    );
    // Élargissement d'action refusé.
    assert!(
        authority
            .attenuate(
                &root,
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Admin],
                Timestamp(chrono::Utc::now() + chrono::Duration::minutes(5)),
            )
            .is_err()
    );
    // Extension temporelle refusée (enfant plus durable que parent).
    assert!(
        authority
            .attenuate(
                &root,
                vec![CapabilityScope::Filesystem],
                vec![CapabilityAction::Read],
                Timestamp(chrono::Utc::now() + chrono::Duration::hours(2)),
            )
            .is_err()
    );
    // Atténuation valide (restriction + expiry plus courte) acceptée.
    let child = authority
        .attenuate(
            &root,
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            Timestamp(chrono::Utc::now() + chrono::Duration::minutes(5)),
        )
        .expect("atténuation valide");
    assert!(
        authority
            .verify(&child, CapabilityScope::Filesystem, CapabilityAction::Read)
            .is_ok()
    );
    // L'enfant restreint ne couvre plus Write.
    assert!(
        authority
            .verify(&child, CapabilityScope::Filesystem, CapabilityAction::Write)
            .is_err()
    );
}

#[test]
fn valid_context_with_quota_authorizes_and_consumes() {
    let authority = CapabilityAuthority::new();
    let token = authority
        .create_root_token(
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            future_ts(),
        )
        .unwrap();
    let mut ctx = AuthorizationContext::new(
        "user:test",
        "annora-kernel",
        future_ts(),
        CapabilityScope::Filesystem,
        CapabilityAction::Read,
        "notes/todo.md",
        Some(Quota::new(1)),
        "nonce-int-1234abcd",
        vec!["user:test".to_string()],
    )
    .unwrap();
    assert!(ctx.authorize(&token, &authority, "annora-kernel").is_ok());
    ctx.try_consume_quota().unwrap();
    // Quota épuisé → validation puis autorisation refusées.
    assert!(ctx.validate().is_err());
    assert!(ctx.authorize(&token, &authority, "annora-kernel").is_err());
}
