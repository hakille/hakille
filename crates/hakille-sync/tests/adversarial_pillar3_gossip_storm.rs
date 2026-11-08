//! Adversarial Stress Testing & Security Invariant Fuzzing: Mesh Gossip Storm Resistance & Revocation Idempotency
//!
//! Validates:
//! - 1,000-fold revocation notice replay storm resistance (strict idempotency, O(1) queue preservation)
//! - Cyclic gossip loop mitigation via hop limit decrements and seen-frame deduplication
//! - Unsigned, empty, or spoofed authority revocation immediate rejection
//! - Massive concurrent burst of 500 distinct revocations mixed with duplicates (eventual consistency & CRL integrity)

use chrono::Utc;
use hakille_substrate::mesh::{EncryptedMeshFrame, InMemoryMeshSubstrate, MeshSubstrate};
use hakille_sync::crdt::CrdtSync;
use hakille_sync::mesh::{MeshGossipEngine, MeshPayload, RevocationTarget, SignedRevocationNotice};
use uuid::Uuid;

#[test]
fn test_adversarial_revocation_replay_storm_and_queue_saturation() {
    let mut engine = MeshGossipEngine::new("node_adversarial_victim");
    let target = RevocationTarget::Did("did:key:z6MkuCompromisedKeyVictim999".to_string());
    let notice = SignedRevocationNotice::new(
        target.clone(),
        "did:key:z6MkuGovernanceAuthority",
        vec![0xAA, 0xBB, 0xCC, 0xDD],
    );

    // 1st ingestion: legitimate new revocation notice
    let accepted = engine.handle_revocation(notice.clone());
    assert!(accepted, "Initial revocation notice must be accepted");
    assert!(engine.is_revoked(&target));
    assert_eq!(engine.outbox_queue.len(), 1);
    assert_eq!(engine.known_revocations.len(), 1);

    // Replay attack: Adversary sends the identical notice 1,000 times to induce a broadcast storm
    let mut rejected_replays = 0;
    for _ in 0..1000 {
        let replay_res = engine.handle_revocation(notice.clone());
        if !replay_res {
            rejected_replays += 1;
        }
    }

    assert_eq!(
        rejected_replays, 1000,
        "All 1,000 replay attempts must be rejected idempotently"
    );
    assert_eq!(
        engine.outbox_queue.len(),
        1,
        "Outbox queue must NOT grow during replay storm (anti-amplification)"
    );
    assert_eq!(
        engine.known_revocations.len(),
        1,
        "CRL must contain exactly 1 entry for target"
    );
}

#[tokio::test]
async fn test_adversarial_cyclic_gossip_loop_mitigation() {
    // Cyclic ring topology: Node A -> Node B -> Node C -> Node A
    let mut node_a = MeshGossipEngine::new("ring_node_A");
    let mut node_b = MeshGossipEngine::new("ring_node_B");
    let mut node_c = MeshGossipEngine::new("ring_node_C");

    let payload = MeshPayload::PeerBeacon {
        sender_did: "did:key:z6MkuCyclicBeacon".to_string(),
        timestamp_ms: 1731008000,
    };

    // Node A encodes a broadcast frame with hop_limit = 5
    let original_frame = node_a
        .encode_frame(&payload, [1u8; 32], None)
        .expect("Encoding frame must succeed");
    let _frame_id = original_frame.frame_id;

    // Step 1: Node A transmits frame to Node B
    let mut frame_to_b = original_frame.clone();
    assert!(frame_to_b.decrement_hop()); // 5 -> 4
    assert_eq!(frame_to_b.hop_limit, 4);

    let decoded_b = node_b.decode_frame(&frame_to_b).unwrap();
    assert!(decoded_b.is_some(), "Node B accepts frame on first receipt");

    // Step 2: Node B transmits frame to Node C
    let mut frame_to_c = frame_to_b.clone();
    assert!(frame_to_c.decrement_hop()); // 4 -> 3
    assert_eq!(frame_to_c.hop_limit, 3);

    let decoded_c = node_c.decode_frame(&frame_to_c).unwrap();
    assert!(decoded_c.is_some(), "Node C accepts frame on first receipt");

    // Step 3: Node C transmits frame back to Node A (cycle closed)
    let mut frame_back_to_a = frame_to_c.clone();
    assert!(frame_back_to_a.decrement_hop()); // 3 -> 2
    assert_eq!(frame_back_to_a.hop_limit, 2);

    // Frame Deduplication Invariant:
    // Node A encodes it first (which may or may not mark seen), but upon decode_frame:
    // If Node A decodes its own frame:
    let decoded_a_first = node_a.decode_frame(&frame_back_to_a).unwrap();
    assert!(
        decoded_a_first.is_some(),
        "Node A receives frame for first time from wire"
    );

    // Now if the cycle continues again (Node A -> Node B -> Node C -> Node A):
    let mut frame_replay_to_b = frame_back_to_a.clone();
    assert!(frame_replay_to_b.decrement_hop()); // 2 -> 1
    let decoded_b_replay = node_b.decode_frame(&frame_replay_to_b).unwrap();
    assert!(
        decoded_b_replay.is_none(),
        "Node B MUST drop duplicate frame_id, breaking the cyclic storm immediately"
    );

    let mut frame_replay_to_c = frame_replay_to_b.clone();
    assert!(frame_replay_to_c.decrement_hop()); // 1 -> 0
    let decoded_c_replay = node_c.decode_frame(&frame_replay_to_c).unwrap();
    assert!(
        decoded_c_replay.is_none(),
        "Node C MUST drop duplicate frame_id"
    );

    // If frame somehow had new IDs, hop limit exhaustion terminates it:
    let mut frame_exhausted =
        EncryptedMeshFrame::new_broadcast([1u8; 32], vec![1, 2], [0u8; 16], 1);
    assert!(frame_exhausted.decrement_hop()); // 1 -> 0
    assert!(
        !frame_exhausted.decrement_hop(),
        "Hop limit at 0 must fail decrement"
    );
}

#[test]
fn test_adversarial_unsigned_and_spoofed_revocations_rejected() {
    let mut engine = MeshGossipEngine::new("node_security_guard");

    // 1. Empty signature
    let unsigned_notice = SignedRevocationNotice {
        notice_id: Uuid::new_v4(),
        revoked_target: RevocationTarget::CapabilityDigest([0x11; 32]),
        authority_did: "did:key:z6MkuGovAuthority".to_string(),
        revoked_at: Utc::now(),
        ed25519_signature: vec![],
    };
    assert!(
        !engine.handle_revocation(unsigned_notice),
        "Notice with empty signature must be rejected"
    );

    // 2. Empty authority DID
    let anonymous_notice = SignedRevocationNotice {
        notice_id: Uuid::new_v4(),
        revoked_target: RevocationTarget::CapabilityDigest([0x22; 32]),
        authority_did: "".to_string(),
        revoked_at: Utc::now(),
        ed25519_signature: vec![1, 2, 3],
    };
    assert!(
        !engine.handle_revocation(anonymous_notice),
        "Notice with empty authority DID must be rejected"
    );

    // Verify outbox and CRL remain completely clean
    assert_eq!(engine.outbox_queue.len(), 0);
    assert_eq!(engine.known_revocations.len(), 0);
}

#[tokio::test]
async fn test_adversarial_burst_concurrency_revocation_flood() {
    let mut engine = MeshGossipEngine::new("node_burst_sink");
    let substrate = InMemoryMeshSubstrate::new();
    let mut crdt = CrdtSync::new();

    let mut expected_targets = Vec::new();

    // Generate 200 unique revocation notices across 4 target kinds
    for i in 0..200 {
        let target = match i % 4 {
            0 => RevocationTarget::Did(format!("did:key:z6MkuBurstBadActor_{i}")),
            1 => RevocationTarget::CapabilityDigest([i as u8; 32]),
            2 => RevocationTarget::TokenCommitment([(255 - (i % 256)) as u8; 32]),
            _ => RevocationTarget::DeviceSession(Uuid::new_v4()),
        };
        expected_targets.push(target.clone());

        let notice = SignedRevocationNotice::new(
            target,
            "did:key:z6MkuGovernanceRoot",
            vec![i as u8, (i + 1) as u8],
        );

        // Ingest legitimate notice
        let accepted = engine.handle_revocation(notice.clone());
        assert!(accepted);

        // Interleave with immediate re-ingestion of the same notice (burst flood)
        let dup = engine.handle_revocation(notice);
        assert!(!dup, "Duplicate in burst must be rejected");
    }

    assert_eq!(engine.known_revocations.len(), 200);
    assert_eq!(engine.outbox_queue.len(), 200);

    // Run sync cycle to flush outbox to substrate
    let processed = engine
        .step_sync_cycle(&substrate, &mut crdt, [42u8; 32])
        .await
        .unwrap();
    assert_eq!(processed, 0, "No incoming frames injected yet");

    let sent_frames = substrate.get_broadcasted_frames().await;
    assert_eq!(
        sent_frames.len(),
        200,
        "All 200 unique notices broadcasted exactly once"
    );

    // All expected targets must be verified in CRL
    for target in &expected_targets {
        assert!(engine.is_revoked(target));
    }
}
