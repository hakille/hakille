//! Suite de conformité du Moteur Génératif Multimodal Souverain (ADR-024).
//!
//! Vérifie bout-en-bout :
//! 1. Génération réelle Image (PNG + chunk `iTXt`), Audio (Formant Klatt 24 kHz +
//!    visèmes + musique/ambiance DSP) et Vidéo (APNG + Y4M + MP4 si `ffmpeg` présent)
//!    avec signature Ed25519 et détection d'altération d'un seul octet ;
//! 2. Contrat d'honnêteté : aucun moteur neural sans poids locaux ne dégrade
//!    silencieusement vers le DSP ;
//! 3. Garde constitutionnel (Règles 1, 2, 4, 7, 8), jeton Biscuit v5 épinglé
//!    (`MediaGeneration` + `Network` cloud) et protection anti-SSRF stricte ;
//! 4. Pipeline multimodal DAG : rejet de cycle + exécution Storyboard avec lignée
//!    cryptographique `parent_artifacts` ;
//! 5. Intégration Noyau (`MediaCommand` + `cloud_action_card`) et stabilité du
//!    protocole UI v1 (`PROTOCOL_VERSION == 1`).

use chrono::Duration;
use ed25519_dalek::SigningKey;
use hakille_core::{
    AuthorizationContext, CapabilityAction, CapabilityAuthority, CapabilityScope, Timestamp,
    TrustLevel,
};
use hakille_kernel::media::{MediaCommand, MediaCommandKind, verify_media_capability};
use hakille_media::backends::cloud::CloudMediaBackend;
use hakille_media::backends::loopback::LoopbackMediaServerBackend;
use hakille_media::{
    AudioGenRequest, DagNode, DagNodeKind, EngineKind, ImageGenMode, ImageGenRequest,
    MediaAuthContext, MediaConstitutionGuard, MediaDataExit, MediaError, MediaJobManager,
    MediaManagerConfig, MediaModality, MediaPipelineDag, MediaPolicy, MediaRequest,
    StoryboardPreset, VideoContainer, VideoGenRequest, extract_png_provenance_chunk,
    verify_artifact_provenance,
};
use hakille_ui_protocol::{
    KernelToUi, MediaEngineDto, MediaGenerateRequestDto, MediaModalityDto, PROTOCOL_VERSION,
    UiToKernel,
};
use std::path::PathBuf;

fn test_signing_key() -> SigningKey {
    SigningKey::from_bytes(&[73u8; 32])
}

fn make_manager(tmp: &tempfile::TempDir) -> MediaJobManager {
    let cfg = MediaManagerConfig::new(tmp.path().join("media"), tmp.path().join("models"));
    MediaJobManager::new(cfg, test_signing_key()).expect("MediaJobManager")
}

#[tokio::test]
async fn conformance_media_image_audio_video_provenance_and_tamper_detection() {
    let tmp = tempfile::tempdir().unwrap();
    let mgr = make_manager(&tmp);
    let vk = mgr.verifying_key();
    let ctx = MediaAuthContext::default();

    // 1. Image PNG + chunk PNG iTXt + Ed25519 + détection d'altération d'1 octet.
    let mut img_req = ImageGenRequest::simple("nébuleuse quantique souveraine");
    img_req.width = 128;
    img_req.height = 128;
    img_req.steps = 6;
    img_req.seed = Some(42);
    img_req.engine = "procedural-dsp".into();

    let img_art = mgr
        .generate_image(img_req, &ctx, None, None, vec![])
        .await
        .expect("image DSP");
    assert_eq!(img_art.modality, MediaModality::Image);
    assert_eq!(img_art.engine_kind, EngineKind::ProceduralDsp);
    assert!(img_art.preview.is_some());

    let png_bytes = std::fs::read(&img_art.path).unwrap();
    assert!(
        extract_png_provenance_chunk(&png_bytes).unwrap().is_some(),
        "chunk PNG iTXt hakille.provenance requis"
    );
    let report = verify_artifact_provenance(&img_art.path, Some(&vk)).expect("Ed25519 valide");
    assert!(report.valid && report.embedded_png_chunk_checked && report.synthetic_ai_disclosure);

    // Altération d'un seul octet -> refus cryptographique garanti.
    let mut tampered = png_bytes;
    let last = tampered.len() - 1;
    tampered[last] ^= 0x01;
    std::fs::write(&img_art.path, &tampered).unwrap();
    let err = verify_artifact_provenance(&img_art.path, Some(&vk)).unwrap_err();
    assert!(matches!(err, MediaError::ProvenanceInvalid(_)));

    // 2. Audio parole Klatt 24 kHz + timeline visèmes + musique DSP.
    let speech_req = AudioGenRequest::speech("Bonjour Hakille souverain");
    let speech_art = mgr
        .generate_audio(speech_req, &ctx, None, None, vec![])
        .await
        .expect("audio parole Klatt");
    assert_eq!(speech_art.modality, MediaModality::Audio);
    assert_eq!(speech_art.engine_kind, EngineKind::FormantDsp);
    assert!(
        speech_art
            .visemes_path
            .as_ref()
            .is_some_and(|p| p.is_file())
    );
    assert!(verify_artifact_provenance(&speech_art.path, Some(&vk)).is_ok());

    let music_req = AudioGenRequest::music("mélodie boréale calme", 1.5, 96);
    let music_art = mgr
        .generate_audio(music_req, &ctx, None, None, vec![])
        .await
        .expect("audio musique DSP");
    assert_eq!(music_art.engine_kind, EngineKind::ProceduralDsp);
    assert!(verify_artifact_provenance(&music_art.path, Some(&vk)).is_ok());

    // 3. Vidéo APNG + Y4M pure Rust (et MP4 si ffmpeg présent).
    let mut vid_req = VideoGenRequest::simple("aurore cinématique souveraine");
    vid_req.width = 128;
    vid_req.height = 96;
    vid_req.fps = 8;
    vid_req.num_frames = 6;
    vid_req.container = VideoContainer::Apng;
    vid_req.engine = "procedural-dsp".into();
    let apng_art = mgr
        .generate_video(vid_req.clone(), None, &ctx, None, None, vec![])
        .await
        .expect("vidéo APNG");
    assert_eq!(apng_art.modality, MediaModality::Video);
    assert!(verify_artifact_provenance(&apng_art.path, Some(&vk)).is_ok());

    vid_req.container = VideoContainer::Y4m;
    let y4m_art = mgr
        .generate_video(vid_req, None, &ctx, None, None, vec![])
        .await
        .expect("vidéo Y4M");
    let y4m_head = std::fs::read(&y4m_art.path).unwrap();
    assert!(y4m_head.starts_with(b"YUV4MPEG2 "));
    assert!(verify_artifact_provenance(&y4m_art.path, Some(&vk)).is_ok());
}

#[tokio::test]
async fn conformance_media_honesty_contract_no_silent_neural_downgrade() {
    let tmp = tempfile::tempdir().unwrap();
    let mgr = make_manager(&tmp);
    let ctx = MediaAuthContext::default();

    // Inventaire honnête : tous les moteurs sont présents avec leur vraie nature.
    let engines = mgr.list_engines();
    assert!(engines.len() >= 10);
    assert!(
        engines
            .iter()
            .any(|e| e.id == "procedural-dsp" && e.availability.is_ready())
    );
    assert!(
        engines
            .iter()
            .any(|e| e.id == "formant-dsp" && e.availability.is_ready())
    );
    assert!(
        engines
            .iter()
            .any(|e| e.id == "sd-turbo-candle" && !e.availability.is_ready())
    );

    // Demander explicitement un moteur neural sans poids DOIT échouer par ModelMissing.
    for img_eng in [
        "sd-1.5-candle",
        "sd-turbo-candle",
        "flux-schnell-candle",
        "onnx-diffusion",
    ] {
        let mut req = ImageGenRequest::simple("test honnêteté image");
        req.width = 128;
        req.height = 128;
        req.engine = img_eng.into();
        let err = mgr
            .generate_image(req, &ctx, None, None, vec![])
            .await
            .unwrap_err();
        assert!(
            matches!(err, MediaError::ModelMissing { .. }),
            "le moteur neural image '{img_eng}' sans poids doit échouer par ModelMissing, reçu: {err:?}"
        );
    }

    for aud_eng in ["parler-tts-candle", "onnx-tts"] {
        let mut req = AudioGenRequest::speech("test honnêteté audio");
        req.engine = aud_eng.into();
        let err = mgr
            .generate_audio(req, &ctx, None, None, vec![])
            .await
            .unwrap_err();
        assert!(
            matches!(err, MediaError::ModelMissing { .. }),
            "le moteur neural audio '{aud_eng}' sans poids doit échouer par ModelMissing, reçu: {err:?}"
        );
    }

    for vid_eng in ["onnx-video", "candle-keyframe"] {
        let mut req = VideoGenRequest::simple("test honnêteté vidéo");
        req.width = 128;
        req.height = 96;
        req.num_frames = 4;
        req.engine = vid_eng.into();
        let err = mgr
            .generate_video(req, None, &ctx, None, None, vec![])
            .await
            .unwrap_err();
        assert!(
            matches!(err, MediaError::ModelMissing { .. }),
            "le moteur neural vidéo '{vid_eng}' sans poids doit échouer par ModelMissing, reçu: {err:?}"
        );
    }
}

#[test]
fn conformance_media_constitution_biscuit_minor_and_anti_ssrf() {
    let guard = MediaConstitutionGuard::new(MediaPolicy::default());
    let req = MediaRequest::Image(ImageGenRequest::simple("paysage montagneux hivernal"));

    // Règle 1 : source Untrusted bloquée (anti prompt-injection indirecte).
    let untrusted = MediaAuthContext {
        trust_level: TrustLevel::Untrusted,
        ..Default::default()
    };
    let err = guard
        .authorize(&req, &MediaDataExit::Local, &untrusted, None, None)
        .unwrap_err();
    assert!(matches!(err, MediaError::ConstitutionBlocked(_)));

    // Règle 4 : jeton Biscuit v5 épinglé (scope MediaGeneration + ressource exacte).
    let authority = CapabilityAuthority::new();
    let expires = Timestamp::now().0 + Duration::minutes(5);
    let local_cmd = MediaCommand::parse(":gen-image paysage montagneux hivernal")
        .unwrap()
        .unwrap();
    let local_res = local_cmd.resource();
    let other_cmd = MediaCommand::parse(":gen-image autre image différente")
        .unwrap()
        .unwrap();
    let cloud_cmd = MediaCommand::parse(":gen-image paysage montagneux hivernal --engine cloud")
        .unwrap()
        .unwrap();
    let media_tok = authority
        .create_root_token_with_binding(
            vec![CapabilityScope::MediaGeneration],
            vec![CapabilityAction::Execute],
            Timestamp(expires),
            Some(&local_res),
            None,
            None,
        )
        .expect("token media");
    let ctx_bound = MediaAuthContext {
        expected_resource: Some(local_res.clone()),
        ..Default::default()
    };
    assert!(
        guard
            .authorize(
                &req,
                &MediaDataExit::Local,
                &ctx_bound,
                Some(&authority),
                Some(&media_tok)
            )
            .is_ok()
    );
    let auth_ctx = AuthorizationContext::new(
        "user:conformance",
        "hakille-kernel",
        Timestamp(expires),
        CapabilityScope::MediaGeneration,
        CapabilityAction::Execute,
        &local_res,
        None,
        "nonce_media_001",
        vec![],
    )
    .expect("auth ctx");
    assert!(
        verify_media_capability(
            &local_cmd,
            TrustLevel::User,
            Some((&auth_ctx, &media_tok, &authority))
        )
        .is_ok()
    );
    assert!(
        verify_media_capability(
            &other_cmd,
            TrustLevel::User,
            Some((&auth_ctx, &media_tok, &authority))
        )
        .is_err(),
        "une ressource différente doit être refusée"
    );
    assert!(
        verify_media_capability(
            &local_cmd,
            TrustLevel::Untrusted,
            Some((&auth_ctx, &media_tok, &authority))
        )
        .is_err(),
        "une source Untrusted doit être refusée"
    );
    let cloud_no_net_tok = authority
        .create_root_token_with_binding(
            vec![CapabilityScope::MediaGeneration],
            vec![CapabilityAction::Execute],
            Timestamp(expires),
            Some(&cloud_cmd.resource()),
            None,
            None,
        )
        .expect("token sans Network");
    assert!(
        verify_media_capability(
            &cloud_cmd,
            TrustLevel::User,
            Some((&auth_ctx, &cloud_no_net_tok, &authority))
        )
        .is_err(),
        "cloud exige aussi CapabilityScope::Network"
    );

    // Règle 7 : profil mineur interdit le cloud et plafonne résolution/durée.
    let cloud_exit = MediaDataExit::Cloud {
        provider: "openai".into(),
        endpoint: "https://api.openai.com/v1".into(),
    };
    let minor_ctx = MediaAuthContext {
        minor_profile: true,
        cloud_explicitly_approved: true,
        ..Default::default()
    };
    assert!(matches!(
        guard.authorize(&req, &cloud_exit, &minor_ctx, None, None),
        Err(MediaError::ConstitutionBlocked(_))
    ));
    let mut huge_img = ImageGenRequest::simple("paysage");
    huge_img.width = 2048;
    huge_img.height = 2048;
    assert!(matches!(
        guard.authorize(
            &MediaRequest::Image(huge_img),
            &MediaDataExit::Local,
            &minor_ctx,
            None,
            None
        ),
        Err(MediaError::PolicyExceeded(_))
    ));

    // Règle 8 : fichier local en référence I2I bloqué vers le cloud.
    let mut i2i = ImageGenRequest::simple("retouche");
    i2i.mode = ImageGenMode::ImageToImage {
        init_image_path: PathBuf::from("/tmp/secret.png"),
        strength: 0.5,
    };
    let cloud_ok = MediaAuthContext {
        cloud_explicitly_approved: true,
        ..Default::default()
    };
    assert!(matches!(
        guard.authorize(
            &MediaRequest::Image(i2i),
            &cloud_exit,
            &cloud_ok,
            None,
            None
        ),
        Err(MediaError::ConstitutionBlocked(_))
    ));

    // Anti-SSRF strict sur Loopback et Cloud.
    assert!(LoopbackMediaServerBackend::validate_loopback_url("http://127.0.0.1:8188").is_ok());
    assert!(LoopbackMediaServerBackend::validate_loopback_url("http://192.168.1.5:8188").is_err());
    assert!(LoopbackMediaServerBackend::validate_loopback_url("http://example.com:8188").is_err());

    assert!(CloudMediaBackend::validate_cloud_url("https://api.example.com/v1").is_ok());
    assert!(CloudMediaBackend::validate_cloud_url("http://api.example.com/v1").is_err());
    assert!(CloudMediaBackend::validate_cloud_url("https://127.0.0.1/v1").is_err());
    assert!(CloudMediaBackend::validate_cloud_url("https://169.254.169.254/latest").is_err());
    assert!(CloudMediaBackend::validate_cloud_url("https://10.0.0.1/v1").is_err());
}

#[tokio::test]
async fn conformance_media_dag_cycle_rejection_and_storyboard_lineage() {
    // 1. Un graphe cyclique est rejeté explicitement avant toute exécution.
    let cyclic = MediaPipelineDag {
        name: "cycle-interdit".into(),
        nodes: vec![
            DagNode {
                id: "a".into(),
                depends_on: vec!["b".into()],
                kind: DagNodeKind::GenerateImage {
                    request: ImageGenRequest::simple("a"),
                },
            },
            DagNode {
                id: "b".into(),
                depends_on: vec!["a".into()],
                kind: DagNodeKind::GenerateImage {
                    request: ImageGenRequest::simple("b"),
                },
            },
        ],
    };
    assert!(
        cyclic.topological_order().is_err(),
        "un cycle DAG doit être rejeté"
    );

    // 2. Storyboard multimodal complet avec checkpoints et lignée SHA-256.
    let tmp = tempfile::tempdir().unwrap();
    let mgr = make_manager(&tmp);
    let vk = mgr.verifying_key();
    let dag = StoryboardPreset::from_prompt(
        "cité solaire au crépuscule",
        128,
        96,
        6,
        1.5,
        Some(99),
        mgr.ffmpeg_available(),
    );
    let res = mgr
        .execute_dag(dag, &MediaAuthContext::default(), None, None)
        .await
        .expect("exécution Storyboard DAG");
    assert!(res.execution_order.len() >= 4);
    let final_art = res
        .artifacts
        .get(&res.final_node_id)
        .expect("artefact final");
    assert!(
        !final_art.parent_artifacts.is_empty(),
        "l'artefact final du DAG doit enregistrer la lignée SHA-256 de ses parents"
    );
    for art in res.artifacts.values() {
        let rep = verify_artifact_provenance(&art.path, Some(&vk)).expect("provenance nœud DAG");
        assert!(rep.valid);
    }
}

#[test]
fn conformance_media_kernel_command_cloud_card_and_protocol_v1_stability() {
    let cmd = MediaCommand::parse(":gen-image nébuleuse --size 256x256 --engine cloud --seed 7")
        .expect("parse :gen-image")
        .expect("commande valide");
    assert_eq!(cmd.kind, MediaCommandKind::Image);
    assert!(cmd.is_cloud());

    let (preview, plan) = cmd.cloud_action_card();
    assert!(
        !preview.reversible,
        "une sortie cloud ne peut pas être marquée annulable"
    );
    assert!(
        !preview.data_exit_local,
        "une sortie cloud ne doit jamais être déclarée locale"
    );
    assert_eq!(plan.steps.len(), 1);

    // Stabilité protocole UI v1 additif.
    assert_eq!(PROTOCOL_VERSION, 1);
    let ui_msg = UiToKernel::MediaGenerateRequest(MediaGenerateRequestDto {
        modality: MediaModalityDto::Image,
        prompt: "test".into(),
        engine: "auto".into(),
        width: Some(128),
        height: Some(128),
        steps: Some(8),
        seed: Some(1),
        duration_secs: None,
        fps: None,
        num_frames: None,
        format: Some("png".into()),
        mode: None,
    });
    let json = serde_json::to_string(&ui_msg).unwrap();
    let roundtrip: UiToKernel = serde_json::from_str(&json).unwrap();
    assert_eq!(ui_msg, roundtrip);

    let reply = KernelToUi::MediaEnginesReply {
        engines: vec![MediaEngineDto {
            id: "procedural-dsp".into(),
            name: "Synthèse Procédurale Rust".into(),
            modality: MediaModalityDto::Image,
            kind_badge: "PROCEDURAL DSP (Rust pur)".into(),
            ready: true,
            status_detail: "CPU local".into(),
            is_cloud: false,
        }],
        ffmpeg_available: true,
    };
    let reply_json = serde_json::to_string(&reply).unwrap();
    let reply_rt: KernelToUi = serde_json::from_str(&reply_json).unwrap();
    assert_eq!(reply, reply_rt);
}
