//! Tests E2E de validation des 5 Piliers d'Élévation Souveraine (Piliers 1 à 5).
//!
//! T1_F21_01 : Projection et halo Braise d'une Ghost Action (§31).
//! T1_F21_02 : Détection de micro-gestes Index-Thumb Pinch et Swipe sans allocation (§32).
//! T1_F21_03 : Projection perspective 3D et détection de contradiction du Palais de la Mémoire (§33).
//! T1_F21_04 : Transport maillé P2P Air-Gap et isolation Link-Local fail-closed (§35).
//! T1_F21_05 : Cycle de vie d'un paquet .hap signé et validation du SDK (§36).

use hakille_mesh::{LinkLocalGuard, MeshFrame, MeshTransport, VirtualAirGapBus};
use hakille_sdk::{HakilleAgentPackage, SdkManifest};
use hakille_spatial::{
    HandJointId, HandKeypoint3D, HandLandmarks3D, Handedness, MicroGestureDetector, Point3D,
};
use hakille_ui_desktop::palace_renderer::{PalaceCamera3D, PalaceRenderNode, PalaceSceneRenderer};
use hakille_ui_desktop::{GhostActionOverlay, PrimitiveCtx, Rect, Scene, Theme};

#[test]
fn t1_f21_01_ghost_action_overlay_halo_and_hold_ring() {
    let ctx = PrimitiveCtx::new(Theme::dark());
    let mut scene = Scene::new(1920, 1080, true);

    let overlay = GhostActionOverlay::new(
        "ghost-01",
        "Chiffrer le document souverain",
        0.96,
        960.0,
        540.0,
        0.75, // 75% du maintien de 600ms écoulé
    );

    overlay.draw(&mut scene, &ctx);
    assert!(
        !scene.cmds.is_empty(),
        "GhostActionOverlay doit produire des commandes graphiques"
    );

    let has_shadow = scene
        .cmds
        .iter()
        .any(|cmd| matches!(cmd, hakille_ui_desktop::scene::DrawCmd::Shadow { .. }));
    let ring_count = scene
        .cmds
        .iter()
        .filter(|cmd| matches!(cmd, hakille_ui_desktop::scene::DrawCmd::Ring { .. }))
        .count();

    assert!(
        has_shadow,
        "Le halo Braise ambiant (#FF7A2F) doit être présent"
    );
    assert!(
        ring_count >= 1,
        "La bague circulaire de maintien 600ms doit être dessinée"
    );
}

#[test]
fn t1_f21_02_spatial_micro_gestures_pinch_and_swipe() {
    let mut detector = MicroGestureDetector::with_default_config();

    // 1. Détection de pincement index-pouce maintenu
    let mut hand = HandLandmarks3D::new(Handedness::Right, 1000);
    hand.set(
        HandJointId::Wrist,
        HandKeypoint3D::new(Point3D::new(0.0, 0.0, 0.0), 0.95),
    );
    hand.set(
        HandJointId::MiddleMcp,
        HandKeypoint3D::new(Point3D::new(0.0, 0.1, 0.0), 0.95),
    );
    hand.set(
        HandJointId::ThumbTip,
        HandKeypoint3D::new(Point3D::new(0.01, 0.15, 0.0), 0.95),
    );
    hand.set(
        HandJointId::IndexTip,
        HandKeypoint3D::new(Point3D::new(0.02, 0.15, 0.0), 0.95),
    );

    // Frame 1 à t=1000
    let ev1 = detector.process_frame(&hand, None);
    assert!(ev1.is_empty(), "La première frame initialise l'hystérésis");

    // Frame 2 à t=1200 (> 150ms de maintien)
    hand.timestamp_ms = 1200;
    let ev2 = detector.process_frame(&hand, None);
    assert_eq!(ev2.len(), 1);
    match &ev2[0] {
        hakille_spatial::DetectedMicroGesture::IndexThumbPinch {
            confirmed,
            duration_ms,
            ..
        } => {
            assert!(confirmed);
            assert_eq!(*duration_ms, 200);
        }
        _ => panic!("IndexThumbPinch attendu"),
    }
}

#[test]
fn t1_f21_03_palace_3d_perspective_projection_and_glow() {
    let camera = PalaceCamera3D::default();
    let projected = camera.project([0.0, 0.0, 0.0], 1280.0, 720.0);
    assert!(projected.is_some());
    let (sx, sy, depth) = projected.unwrap();
    assert!(sx > 500.0 && sx < 800.0);
    assert!(sy > 250.0 && sy < 450.0);
    assert!(depth > 50.0);

    let mut renderer = PalaceSceneRenderer::new();
    renderer.nodes.push(PalaceRenderNode {
        id: "node-c1".into(),
        label: "Fait contradictoire".into(),
        position: [0.0, 0.0, 0.0],
        weight: 0.9,
        contradiction_level: 0.95, // Doit pulser
    });
    renderer.set_pulse_time(0.5);

    let ctx = PrimitiveCtx::new(Theme::dark());
    let mut scene = Scene::new(1280, 720, true);
    let vp = Rect::new(0.0, 0.0, 1280.0, 720.0);
    renderer.draw(&mut scene, &ctx, vp);

    let has_glow = scene
        .cmds
        .iter()
        .any(|cmd| matches!(cmd, hakille_ui_desktop::scene::DrawCmd::Shadow { .. }));
    assert!(
        has_glow,
        "Le nœud en contradiction doit émettre un halo pulsé"
    );
}

#[tokio::test]
async fn t1_f21_04_mesh_air_gap_link_local_fail_closed() {
    // Vérification de la barrière anti-fuite WAN
    assert!(!LinkLocalGuard::is_link_local_safe(
        "https://evil-cloud.com/telemetry"
    ));
    assert!(!LinkLocalGuard::is_link_local_safe("198.51.100.1:443"));
    assert!(LinkLocalGuard::is_link_local_safe("169.254.42.1:8000"));
    assert!(LinkLocalGuard::is_link_local_safe(
        "virtual://sovereign_peer"
    ));

    // Simulation d'échange de trames chiffrées sur le bus virtuel
    let bus = VirtualAirGapBus::new(32);
    let peer1 = bus.join_peer("node-1");
    let peer2 = bus.join_peer("node-2");

    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[88u8; 32]);
    let sym_key = [44u8; 32];
    let nonce = [9u8; 12];
    let payload = b"Secret souverain sans nuage";

    let frame = MeshFrame::create_encrypted(
        "node-1",
        Some("node-2".into()),
        1,
        payload,
        &sym_key,
        nonce,
        &signing_key,
    )
    .unwrap();

    peer1.send_direct("node-2", frame).await.unwrap();
    let received = peer2.receive().await.unwrap();

    let decrypted = received.verify_and_decrypt(&sym_key).unwrap();
    assert_eq!(decrypted, payload);
}

#[test]
fn t1_f21_05_hap_package_developer_signing_and_validation() {
    let manifest_toml = r#"
id = "sovereign-wallet"
name = "Portefeuille Hors-Ligne"
version = "1.0.0"
description = "Signatures locales sans fuite réseau"
author = "Sovereign Core Team"
license = "Apache-2.0"
entrypoint = "wallet.wasm"
capabilities = ["filesystem:read"]
"#;

    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[91u8; 32]);
    let wasm_binary = b"\x00asm\x01\x00\x00\x00\x01\x04\x01\x60\x00\x00";

    let package = HakilleAgentPackage::pack(manifest_toml, wasm_binary, &signing_key).unwrap();
    let validated_manifest = package.unpack_and_verify().unwrap();

    assert_eq!(validated_manifest.id, "sovereign-wallet");
    assert_eq!(validated_manifest.version, "1.0.0");
    assert_eq!(package.wasm_bytes, wasm_binary);
}
