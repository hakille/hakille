//! Feature 06: Honest Wait & Input Locking (Milestone M1 / AGENTS.md, Spec §6)
//!
//! Specifications: T1_F06_01 to T1_F06_05
//!
//! Verifies:
//! - Single in-flight request constraint: input locking strictly prevents concurrent dispatches.
//! - Monotonic request ID generation and reserved barrier ID protection.
//! - Deterministic input unlocking immediately upon terminal response arrival.
//! - Honest transport drop handling: in-flight mutating requests enter unknown outcome with recovery_required.
//! - Non-mutating requests (e.g. status) fail gracefully without triggering recovery lock.

use crate::harness::{DaemonStep, MobileTestRig};
use hakille_mobile::MobileBridge;
use hakille_mobile_gateway::BARRIER_ID;
use serde_json::json;

/// `T1_F06_01`: Input locking during in-flight request.
///
/// While `snapshot.pending` is populated, any subsequent mutating or query dispatch
/// is strictly rejected with "Une requête est déjà en cours".
#[test]
fn test_t1_f06_01_input_locking_during_in_flight_request() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080",
        })
        .to_string(),
    );
    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Première requête en vol",
        })
        .to_string(),
    );

    // 1. First dispatch succeeds and enters in-flight state
    let res1_str = bridge.dispatch_json(json!({"type": "submit"}).to_string());
    let res1: serde_json::Value = serde_json::from_str(&res1_str).unwrap();
    assert_eq!(res1["ok"], true);
    assert_eq!(res1["snapshot"]["pending"]["kind"], "submit");

    // 2. Second dispatch while in-flight is rejected
    let res2_str = bridge.dispatch_json(json!({"type": "refresh_status"}).to_string());
    let res2: serde_json::Value = serde_json::from_str(&res2_str).unwrap();
    assert_eq!(res2["ok"], false);
    assert_eq!(
        res2["error"], "Une requête est déjà en cours",
        "Concurrent dispatch must be locked: {res2_str}"
    );

    // 3. Attempting another submit also rejected
    let res3_str = bridge.dispatch_json(json!({"type": "submit"}).to_string());
    let res3: serde_json::Value = serde_json::from_str(&res3_str).unwrap();
    assert_eq!(res3["ok"], false);
    assert_eq!(res3["error"], "Une requête est déjà en cours");
}

/// `T1_F06_02`: Strictly monotonic request IDs and barrier ID reservation.
///
/// Successive requests strictly increment request IDs. The value u64::MAX
/// is reserved for the gateway barrier frame and can never be issued to client dispatches.
#[tokio::test]
async fn test_t1_f06_02_strictly_monotonic_request_ids_and_barrier_reservation() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    assert_eq!(BARRIER_ID, u64::MAX, "BARRIER_ID must equal u64::MAX");

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        // Handle step 1 (status) and step 2 (status)
        daemon
            .handle_step(&mut stream, 1, DaemonStep::status())
            .await
            .expect("step 1");
        daemon
            .handle_step(&mut stream, 2, DaemonStep::status())
            .await
            .expect("step 2");
    });

    let (s1, r1) = rig
        .roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;
    assert_eq!(s1, 200);
    let id1 = r1["snapshot"]["outcomes"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()["request_id"]
        .as_u64()
        .unwrap();
    assert_eq!(id1, 1);

    let (s2, r2) = rig
        .roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;
    assert_eq!(s2, 200);
    let id2 = r2["snapshot"]["outcomes"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()["request_id"]
        .as_u64()
        .unwrap();
    assert_eq!(id2, 2);

    assert!(
        id2 > id1,
        "Request IDs must be strictly monotonic: {id2} > {id1}"
    );
    assert_ne!(id1, BARRIER_ID);
    assert_ne!(id2, BARRIER_ID);

    daemon_task.await.unwrap();
}

/// `T1_F06_03`: Deterministic input unlocking upon terminal response arrival.
///
/// Once receive_json processes the terminal response, snapshot.pending is reset to None,
/// permitting subsequent dispatches without delay.
#[tokio::test]
async fn test_t1_f06_03_input_unlocking_upon_terminal_response() {
    let (rig, daemon) = MobileTestRig::setup().await;
    let bridge = rig.new_bridge();

    let daemon_task = tokio::spawn(async move {
        let mut stream = daemon.accept().await.expect("accept");
        daemon
            .handle_step(&mut stream, 1, DaemonStep::status())
            .await
            .expect("step 1");
    });

    // Before send, pending is None
    let snap_before = rig.snapshot(&bridge);
    assert!(snap_before["pending"].is_null());

    let (status, res) = rig
        .roundtrip(&bridge, json!({"type": "refresh_status"}))
        .await;
    assert_eq!(status, 200);
    assert_eq!(res["ok"], true);

    // After terminal roundtrip, pending must be cleanly unlocked
    let snap_after = rig.snapshot(&bridge);
    assert!(
        snap_after["pending"].is_null(),
        "Pending must be cleared upon terminal receipt"
    );

    daemon_task.await.unwrap();
}

/// `T1_F06_04`: Honest transport drop handling for mutating requests.
///
/// On transport drop during a mutating action, fail_request marks outcome as "unknown",
/// clears pending, and sets recovery_required = true to prevent automatic retries.
#[test]
fn test_t1_f06_04_honest_wait_failure_on_transport_drop() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080",
        })
        .to_string(),
    );
    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Action mutante interrompue",
        })
        .to_string(),
    );

    let submit_res_str = bridge.dispatch_json(json!({"type": "submit"}).to_string());
    let submit_res: serde_json::Value = serde_json::from_str(&submit_res_str).unwrap();
    let req_id = submit_res["request"]["id"].as_u64().unwrap();

    // Simulate network drop
    let fail_res_str = bridge.fail_request(req_id, "Socket dropped by host OS".into());
    let fail_res: serde_json::Value = serde_json::from_str(&fail_res_str).unwrap();
    assert_eq!(fail_res["ok"], true);

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert!(snap["pending"].is_null(), "Pending lock must be released");
    assert_eq!(
        snap["recovery_required"], true,
        "Mutating failure must require recovery acknowledgement"
    );

    let last_outcome = snap["outcomes"].as_array().unwrap().last().unwrap();
    assert_eq!(
        last_outcome["state"], "unknown",
        "Mutating failure must be marked 'unknown', never 'confirmed'"
    );

    // Subsequent mutation must be blocked by recovery_required
    bridge.dispatch_json(
        json!({
            "type": "set_draft",
            "text": "Tentative bloquée",
        })
        .to_string(),
    );
    let blocked_res_str = bridge.dispatch_json(json!({"type": "submit"}).to_string());
    assert!(
        blocked_res_str.contains("Résultat précédent inconnu")
            || blocked_res_str.contains("Opération précédente incertaine"),
        "Must block subsequent mutation: {blocked_res_str}"
    );
}

/// `T1_F06_05`: Non-mutating requests fail gracefully without recovery lock.
///
/// Failing a read-only query (e.g. refresh_status) records outcome "failed",
/// but does NOT set recovery_required = true.
#[test]
fn test_t1_f06_05_non_mutating_requests_do_not_require_recovery() {
    let bridge = MobileBridge::new();
    bridge.dispatch_json(
        json!({
            "type": "set_hub",
            "hub_id": "http://127.0.0.1:8080",
        })
        .to_string(),
    );

    let query_res_str = bridge.dispatch_json(json!({"type": "refresh_status"}).to_string());
    let query_res: serde_json::Value = serde_json::from_str(&query_res_str).unwrap();
    let req_id = query_res["request"]["id"].as_u64().unwrap();

    // Fail query
    bridge.fail_request(req_id, "DNS resolution failed".into());

    let snap: serde_json::Value = serde_json::from_str(&bridge.snapshot_json()).unwrap();
    assert_eq!(
        snap["recovery_required"], false,
        "Read-only failures must not lock the bridge for recovery"
    );

    let last_outcome = snap["outcomes"].as_array().unwrap().last().unwrap();
    assert_eq!(
        last_outcome["state"], "failed",
        "Non-mutating failure is marked 'failed'"
    );
}
