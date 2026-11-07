//! Adversarial Stress Testing & Invariant Verification: Digital Twilight Key Shredding & RAM Zeroization
//!
//! Validates:
//! - Complete zeroization of all 5 cryptographic subkeys in RAM (ZeroizeOnDrop / zeroize())
//! - Irrevocable shredding of partition salts in RedbZkStore
//! - Transition to AccountStatus::DigitalTwilightGrace
//! - Proof that zeroized RAM keys strictly fail to authenticate/decrypt previous ciphertext
//! - Decoy workspace isolation (vault locked, simulated balance, innocuous documents)
//! - Mesh silent distress beacon invariance (256B constant size, traffic-analysis resistant)

use chrono::Utc;
use hakille_auth::silent_distress::SilentDistressCoordinator;
use hakille_core::crypto::aead::XChaChaCipher;
use hakille_core::{HakilleError, UserId};
use hakille_identity::crypto::{SubkeyHierarchy, compute_blind_index};
use hakille_identity::ssi::Did;
use hakille_identity::storage::{AccountStatus, RedbZkStore, UserRecord};
use uuid::Uuid;

#[test]
fn test_adversarial_digital_twilight_complete_ram_zeroization_and_cryptographic_invalidation() {
    let mut zk_store = RedbZkStore::new();
    let user_id = UserId(Uuid::new_v4());
    let did = Did::new("did:key:z6MkuTwilightOwnerKey789");

    let blind_key = [0x55u8; 32];
    let username = "sovereign_owner";
    let blind_hash = compute_blind_index(username, &blind_key);

    let initial_user = UserRecord {
        user_id: user_id.clone(),
        primary_did: did,
        blind_username_hash: blind_hash,
        encrypted_profile_blob: hakille_identity::crypto::XChaChaBlob {
            nonce: [1u8; 24],
            ciphertext: vec![10, 20, 30],
            tag: [0u8; 16],
            aad_fingerprint: [0u8; 16],
        },
        state: AccountStatus::Active,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    zk_store.insert_user(initial_user).unwrap();

    // Verify initial state: salt NOT shredded
    assert!(!zk_store.is_salt_shredded(&user_id));

    // Initialize non-zero subkeys
    let original_memory_key = [0xAAu8; 32];
    let mut subkeys = SubkeyHierarchy {
        identity_key: [0x11u8; 32],
        memory_key: original_memory_key,
        credentials_key: [0x33u8; 32],
        biometrics_key: [0x44u8; 32],
        blind_index_key: blind_key,
    };

    // Encrypt sensitive secret memory payload before distress
    let secret_plaintext = b"CONFIDENTIAL_SOVEREIGN_PASSPHRASE_AND_BIOMETRICS";
    let aad = b"context:financial_vault";
    let encrypted_blob = XChaChaCipher::encrypt(secret_plaintext, &original_memory_key, aad)
        .expect("Encryption with valid key must succeed");

    // Verify legitimate decryption works before twilight
    let decrypted_before = XChaChaCipher::decrypt(&encrypted_blob, &original_memory_key, aad)
        .expect("Decryption with original key must succeed");
    assert_eq!(decrypted_before, secret_plaintext);

    // =========================================================================
    // EXECUTE DIGITAL TWILIGHT PROTOCOL
    // =========================================================================
    let (payload, decoy) = SilentDistressCoordinator::execute_distress_protocol_with_decoy(
        &user_id,
        &mut subkeys,
        &mut zk_store,
    )
    .expect("Distress protocol execution must succeed");

    // 1. Invariant: Persistent storage partition salt is shredded
    assert!(
        zk_store.is_salt_shredded(&user_id),
        "Partition salt in RedbZkStore must be marked shredded"
    );

    // 2. Invariant: User account state transitioned to DigitalTwilightGrace
    let updated_user = zk_store
        .lookup_user_by_blind_username(username, &blind_key)
        .unwrap();
    assert_eq!(
        updated_user.state,
        AccountStatus::DigitalTwilightGrace,
        "Account must be in DigitalTwilightGrace state"
    );

    // 3. Invariant: ALL 5 subkeys in RAM are zeroized to [0u8; 32]
    assert_eq!(
        *subkeys.identity_key(),
        [0u8; 32],
        "identity_key in RAM must be completely zeroized"
    );
    assert_eq!(
        *subkeys.memory_key(),
        [0u8; 32],
        "memory_key in RAM must be completely zeroized"
    );
    assert_eq!(
        *subkeys.credentials_key(),
        [0u8; 32],
        "credentials_key in RAM must be completely zeroized"
    );
    assert_eq!(
        *subkeys.biometrics_key(),
        [0u8; 32],
        "biometrics_key in RAM must be completely zeroized"
    );
    assert_eq!(
        *subkeys.blind_index_key(),
        [0u8; 32],
        "blind_index_key in RAM must be completely zeroized"
    );

    // 4. Invariant: Decryption using the zeroized memory_key in RAM strictly fails
    let decrypt_attempt = XChaChaCipher::decrypt(&encrypted_blob, subkeys.memory_key(), aad);
    assert!(
        decrypt_attempt.is_err(),
        "Attempting decryption with zeroized RAM key must fail authentication tag check"
    );

    // 5. Invariant: Decoy environment isolation
    assert!(
        decoy.vault_access_locked,
        "Decoy workspace must strictly lock vault access"
    );
    assert_eq!(
        decoy.simulated_balance_cents, 24_000,
        "Simulated balance must be modest (240.00 EUR)"
    );
    assert!(
        decoy
            .decoy_documents
            .iter()
            .all(|doc| !doc.contains("vault") && !doc.contains("secret")),
        "Decoy documents must be innocuous"
    );

    // 6. Invariant: Mesh alert stealth and size padding (traffic analysis resistance)
    assert!(payload.is_silent, "Mesh distress alert must be silent");
    assert_eq!(
        payload.encrypted_mesh_telemetry.len(),
        256,
        "Telemetry payload must be strictly padded to 256 bytes"
    );
    assert_eq!(payload.mesh_channel, "hakille.mesh.sovereign.distress");
    assert_eq!(payload.user_id, user_id);
}

#[test]
fn test_adversarial_digital_twilight_idempotent_double_trigger() {
    let mut zk_store = RedbZkStore::new();
    let user_id = UserId(Uuid::new_v4());
    let did = Did::new("did:key:z6MkuIdempotentTwilight");

    let user = UserRecord {
        user_id: user_id.clone(),
        primary_did: did,
        blind_username_hash: [0x77u8; 32],
        encrypted_profile_blob: hakille_identity::crypto::XChaChaBlob {
            nonce: [2u8; 24],
            ciphertext: vec![1],
            tag: [0u8; 16],
            aad_fingerprint: [0u8; 16],
        },
        state: AccountStatus::Active,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    zk_store.insert_user(user).unwrap();

    let mut subkeys1 = SubkeyHierarchy {
        identity_key: [1u8; 32],
        memory_key: [2u8; 32],
        credentials_key: [3u8; 32],
        biometrics_key: [4u8; 32],
        blind_index_key: [5u8; 32],
    };

    // First trigger
    let res1 = SilentDistressCoordinator::execute_distress_protocol(
        &user_id,
        &mut subkeys1,
        &mut zk_store,
    );
    assert!(res1.is_ok());
    assert!(zk_store.is_salt_shredded(&user_id));

    // Second trigger (attacker or repeated distress blink)
    let mut subkeys2 = SubkeyHierarchy {
        identity_key: [6u8; 32],
        memory_key: [7u8; 32],
        credentials_key: [8u8; 32],
        biometrics_key: [9u8; 32],
        blind_index_key: [10u8; 32],
    };
    let res2 = SilentDistressCoordinator::execute_distress_protocol(
        &user_id,
        &mut subkeys2,
        &mut zk_store,
    );
    assert!(res2.is_ok(), "Repeated distress trigger must be idempotent");
    assert!(zk_store.is_salt_shredded(&user_id));
    assert_eq!(*subkeys2.identity_key(), [0u8; 32]);
}

#[test]
fn test_adversarial_digital_twilight_unknown_user_rejection() {
    let mut zk_store = RedbZkStore::new();
    let unknown_user = UserId(Uuid::new_v4());
    let mut subkeys = SubkeyHierarchy {
        identity_key: [1u8; 32],
        memory_key: [2u8; 32],
        credentials_key: [3u8; 32],
        biometrics_key: [4u8; 32],
        blind_index_key: [5u8; 32],
    };

    let res = SilentDistressCoordinator::execute_distress_protocol(
        &unknown_user,
        &mut subkeys,
        &mut zk_store,
    );
    assert!(res.is_err());
    match res.unwrap_err() {
        HakilleError::AuthenticationFailed => {}
        err => panic!("Expected AuthenticationFailed, got {:?}", err),
    }
}
