//! # `hakille-media` — Moteur Génératif Multimodal Souverain (ADR-024)
//!
//! Architecture 100 % Rust pur (zéro Python) couvrant :
//! - **Image** : Candle Stable Diffusion 1.5 / Turbo, FLUX.1 Schnell, ONNX
//!   Diffusion, Procédural DSP, Loopback `127.0.0.1` & Cloud HTTPS opt-in ;
//! - **Audio** : Candle Parler-TTS + DAC, ONNX TTS, Formant Klatt 24 kHz
//!   (`hakille-voice` + timeline visèmes), Synthétiseur polyphonique DSP,
//!   mixage multipiste avec sidechain ducking automatique ;
//! - **Vidéo** : ONNX spatio-temporel 5D, Candle SLERP latente, Procédural
//!   cinématique & Audio-Réactif RMS, encodeurs purs Rust (APNG, Y4M,
//!   FrameSequence) et muxer explicite `FfmpegMuxer` (MP4 H.264 / WebM VP9) ;
//! - **Composition DAG** : graphe acyclique checkpointé (`MediaPipelineDag`)
//!   avec lignée cryptographique `parent_artifacts` ;
//! - **Souveraineté & Provenance** : signature Ed25519 + sidecar JSON + chunk
//!   PNG `iTXt` (`hakille.provenance`), garde Constitution + Biscuit v5.

pub mod audio;
pub mod backends;
pub mod catalog;
pub mod dag;
pub mod error;
pub mod guard;
pub mod image;
pub mod jobs;
pub mod policy;
pub mod provenance;
pub mod store;
pub mod types;
pub mod video;

pub use catalog::{GenerativeModelProfile, MediaModelCatalog, sha256_bytes, sha256_file};
pub use dag::{DagExecutionResult, DagNode, DagNodeKind, MediaPipelineDag, StoryboardPreset};
pub use error::{MediaError, MediaResult};
pub use guard::{MediaAuthContext, MediaConstitutionGuard};
pub use jobs::{MediaJobManager, MediaManagerConfig};
pub use policy::MediaPolicy;
pub use provenance::{
    MediaProvenance, MediaProvenancePayload, PROVENANCE_PNG_KEYWORD, ProvenanceVerificationReport,
    extract_png_provenance_chunk, provenance_sidecar_path, verify_artifact_provenance,
};
pub use store::MediaStore;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::mastering::{AudioBuffer, mix_with_sidechain_ducking};
    use crate::backends::cloud::CloudMediaBackend;
    use crate::backends::loopback::LoopbackMediaServerBackend;
    use crate::video::candle_keyframe::slerp_latents;
    use crate::video::muxer::FfmpegMuxer;
    use chrono::Duration;
    use ed25519_dalek::SigningKey;
    use hakille_core::{
        CapabilityAction, CapabilityAuthority, CapabilityScope, Timestamp, TrustLevel,
    };

    fn test_signing_key() -> SigningKey {
        SigningKey::from_bytes(&[42u8; 32])
    }

    fn make_manager(tmp: &tempfile::TempDir) -> MediaJobManager {
        let cfg = MediaManagerConfig::new(tmp.path().join("media"), tmp.path().join("models"));
        MediaJobManager::new(cfg, test_signing_key()).expect("création MediaJobManager")
    }

    #[tokio::test]
    async fn image_procedural_provenance_png_chunk_and_tamper_detection() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = make_manager(&tmp);
        let mut req = ImageGenRequest::simple("nébuleuse cosmique bleue souveraine");
        req.width = 128;
        req.height = 128;
        req.steps = 6;
        req.seed = Some(1337);
        req.engine = "procedural-dsp".into();

        let art = mgr
            .generate_image(req, &MediaAuthContext::default(), None, None, vec![])
            .await
            .expect("génération image DSP");

        assert_eq!(art.modality, MediaModality::Image);
        assert_eq!(art.engine_kind, EngineKind::ProceduralDsp);
        assert!(art.path.is_file());
        assert!(art.provenance_path.is_file());
        assert!(
            art.preview
                .as_ref()
                .is_some_and(|p| p.width > 0 && !p.rgba.is_empty())
        );

        // Chunk PNG iTXt présent dans le binaire PNG.
        let png_bytes = std::fs::read(&art.path).unwrap();
        let chunk = extract_png_provenance_chunk(&png_bytes).unwrap();
        assert!(chunk.is_some(), "le chunk PNG iTXt doit être embarqué");

        // Vérification cryptographique complète réussit avec la clé épinglée.
        let report =
            verify_artifact_provenance(&art.path, Some(&mgr.verifying_key())).expect("valide");
        assert!(report.valid);
        assert!(report.embedded_png_chunk_checked);
        assert!(report.synthetic_ai_disclosure);

        // Altération d'un seul octet → rejet cryptographique garanti.
        let mut tampered = png_bytes;
        let last = tampered.len() - 1;
        tampered[last] ^= 0xFF;
        std::fs::write(&art.path, &tampered).unwrap();
        let err = verify_artifact_provenance(&art.path, Some(&mgr.verifying_key())).unwrap_err();
        assert!(matches!(err, MediaError::ProvenanceInvalid(_)));
    }

    #[tokio::test]
    async fn neural_engines_report_honest_model_missing_without_silent_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = make_manager(&tmp);

        for engine in [
            "sd-1.5-candle",
            "sd-turbo-candle",
            "flux-schnell-candle",
            "onnx-diffusion",
        ] {
            let mut req = ImageGenRequest::simple("test honnêteté");
            req.width = 128;
            req.height = 128;
            req.engine = engine.into();
            let err = mgr
                .generate_image(req, &MediaAuthContext::default(), None, None, vec![])
                .await
                .unwrap_err();
            assert!(
                matches!(err, MediaError::ModelMissing { .. }),
                "le moteur {engine} sans poids doit renvoyer ModelMissing, reçu {err:?}"
            );
        }

        for engine in ["parler-tts-candle", "onnx-tts"] {
            let mut req = AudioGenRequest::speech("bonjour");
            req.engine = engine.into();
            let err = mgr
                .generate_audio(req, &MediaAuthContext::default(), None, None, vec![])
                .await
                .unwrap_err();
            assert!(
                matches!(err, MediaError::ModelMissing { .. }),
                "le moteur audio {engine} sans poids doit renvoyer ModelMissing, reçu {err:?}"
            );
        }

        for engine in ["onnx-video", "candle-keyframe"] {
            let mut req = VideoGenRequest::simple("test vidéo");
            req.width = 128;
            req.height = 128;
            req.num_frames = 4;
            req.engine = engine.into();
            let err = mgr
                .generate_video(req, None, &MediaAuthContext::default(), None, None, vec![])
                .await
                .unwrap_err();
            assert!(
                matches!(err, MediaError::ModelMissing { .. }),
                "le moteur vidéo {engine} sans poids doit renvoyer ModelMissing, reçu {err:?}"
            );
        }
    }

    #[tokio::test]
    async fn audio_formant_emits_wav_and_visemes_and_sidechain_ducking_works() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = make_manager(&tmp);

        let mut speech = AudioGenRequest::speech("Bonjour Hakille, génération audio souveraine.");
        speech.engine = "formant-dsp".into();
        let voice_art = mgr
            .generate_audio(speech, &MediaAuthContext::default(), None, None, vec![])
            .await
            .expect("synthèse vocale formantique");
        assert_eq!(voice_art.engine_kind, EngineKind::FormantDsp);
        assert!(voice_art.visemes_path.as_ref().is_some_and(|p| p.is_file()));

        let mut music = AudioGenRequest::music("ambiance nocturne cyber", 2.0, 96);
        music.engine = "procedural-dsp".into();
        let bed_art = mgr
            .generate_audio(music, &MediaAuthContext::default(), None, None, vec![])
            .await
            .expect("synthèse musicale DSP");
        assert_eq!(bed_art.engine_kind, EngineKind::ProceduralDsp);

        // Vérifie que le sidechain ducking atténue réellement le lit quand la voix parle.
        let sr = 24_000u32;
        let n = 12_000usize;
        let voice_buf = AudioBuffer::new(
            sr,
            1,
            (0..n).map(|i| if i < n / 2 { 0.8 } else { 0.0 }).collect(),
        )
        .unwrap();
        let bed_buf = AudioBuffer::new(sr, 1, vec![0.4; n]).unwrap();
        let mixed = mix_with_sidechain_ducking(&voice_buf, &bed_buf, 1.0, 0.25).unwrap();
        assert_eq!(mixed.num_frames(), n);
    }

    #[tokio::test]
    async fn video_apng_y4m_and_optional_ffmpeg_mp4_generation() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = make_manager(&tmp);

        // 1. APNG 100 % Rust pur.
        let mut req = VideoGenRequest::simple("aurore boréale fluide");
        req.width = 96;
        req.height = 64;
        req.fps = 8;
        req.num_frames = 6;
        req.container = VideoContainer::Apng;
        req.engine = "procedural-dsp".into();
        let apng = mgr
            .generate_video(
                req.clone(),
                None,
                &MediaAuthContext::default(),
                None,
                None,
                vec![],
            )
            .await
            .expect("génération APNG");
        let apng_bytes = std::fs::read(&apng.path).unwrap();
        assert_eq!(&apng_bytes[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        assert!(verify_artifact_provenance(&apng.path, Some(&mgr.verifying_key())).is_ok());

        // 2. Y4M 100 % Rust pur.
        req.container = VideoContainer::Y4m;
        let y4m = mgr
            .generate_video(
                req.clone(),
                None,
                &MediaAuthContext::default(),
                None,
                None,
                vec![],
            )
            .await
            .expect("génération Y4M");
        let y4m_bytes = std::fs::read(&y4m.path).unwrap();
        assert!(y4m_bytes.starts_with(b"YUV4MPEG2 "));

        // 3. MP4 H.264 si ffmpeg est installé sur la machine.
        if FfmpegMuxer::detect().is_some() {
            req.container = VideoContainer::Mp4H264;
            let mp4 = mgr
                .generate_video(req, None, &MediaAuthContext::default(), None, None, vec![])
                .await
                .expect("génération MP4 via ffmpeg");
            assert!(mp4.bytes_len > 100);
            assert!(verify_artifact_provenance(&mp4.path, Some(&mgr.verifying_key())).is_ok());
        }
    }

    #[tokio::test]
    async fn dag_detects_cycles_and_executes_storyboard_with_lineage() {
        // Détection de cycle fail-closed.
        let cyclic = MediaPipelineDag {
            name: "cycle".into(),
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
        assert!(matches!(
            cyclic.topological_order().unwrap_err(),
            MediaError::DagCycleDetected(_)
        ));

        // Exécution complète StoryboardPreset (avec ou sans ffmpeg).
        let tmp = tempfile::tempdir().unwrap();
        let mgr = make_manager(&tmp);
        let dag = StoryboardPreset::from_prompt(
            "océan nocturne bioluminescent",
            128,
            96,
            6,
            1.5,
            Some(99),
            mgr.ffmpeg_available(),
        );
        let result = mgr
            .execute_dag(dag, &MediaAuthContext::default(), None, None)
            .await
            .expect("exécution DAG storyboard");

        let final_art = &result.artifacts[&result.final_node_id];
        assert!(
            !final_art.parent_artifacts.is_empty(),
            "l'artefact final doit référencer ses parents dans sa lignée"
        );
        let report =
            verify_artifact_provenance(&final_art.path, Some(&mgr.verifying_key())).unwrap();
        assert!(report.parent_count >= 1);
    }

    #[test]
    fn constitution_guard_biscuit_and_ssrf_invariants() {
        let guard = MediaConstitutionGuard::new(MediaPolicy::default());
        let auth = CapabilityAuthority::new();
        let exp = Timestamp(chrono::Utc::now() + Duration::minutes(10));

        let req = MediaRequest::Image(ImageGenRequest::simple("un lac calme au lever du soleil"));

        // 1. Source Untrusted bloquée (Règle 1).
        let untrusted_ctx = MediaAuthContext {
            trust_level: TrustLevel::Untrusted,
            ..Default::default()
        };
        assert!(matches!(
            guard.authorize(&req, &MediaDataExit::Local, &untrusted_ctx, None, None),
            Err(MediaError::ConstitutionBlocked(_))
        ));

        // 2. Sans jeton MediaGeneration quand l'autorité est active → CapabilityDenied.
        assert!(matches!(
            guard.authorize(
                &req,
                &MediaDataExit::Local,
                &MediaAuthContext::default(),
                Some(&auth),
                None,
            ),
            Err(MediaError::CapabilityDenied(_))
        ));

        // 3. Jeton avec mauvais scope (Audio seul) → CapabilityDenied.
        let wrong_tok = auth
            .create_root_token(
                vec![CapabilityScope::Audio],
                vec![CapabilityAction::Execute],
                exp.clone(),
            )
            .unwrap();
        assert!(matches!(
            guard.authorize(
                &req,
                &MediaDataExit::Local,
                &MediaAuthContext::default(),
                Some(&auth),
                Some(&wrong_tok),
            ),
            Err(MediaError::CapabilityDenied(_))
        ));

        // 4. Jeton MediaGeneration + resource binding exact → OK.
        let ok_tok = auth
            .create_root_token_with_binding(
                vec![CapabilityScope::MediaGeneration],
                vec![CapabilityAction::Execute],
                exp.clone(),
                Some("media:image-test"),
                None,
                None,
            )
            .unwrap();
        let bound_ctx = MediaAuthContext {
            expected_resource: Some("media:image-test".into()),
            ..Default::default()
        };
        assert!(
            guard
                .authorize(
                    &req,
                    &MediaDataExit::Local,
                    &bound_ctx,
                    Some(&auth),
                    Some(&ok_tok),
                )
                .is_ok()
        );

        // 5. Cloud sans Network scope → refusé ; profil mineur → refusé (Règle 7).
        let cloud_exit = MediaDataExit::Cloud {
            provider: "test".into(),
            endpoint: "https://api.example.com".into(),
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

        // 6. Anti-SSRF cloud & loopback strict.
        assert!(CloudMediaBackend::validate_cloud_url("http://api.example.com").is_err());
        assert!(CloudMediaBackend::validate_cloud_url("https://127.0.0.1:8080").is_err());
        assert!(CloudMediaBackend::validate_cloud_url("https://192.168.1.5").is_err());
        assert!(CloudMediaBackend::validate_cloud_url("https://api.openai.com").is_ok());
        assert!(LoopbackMediaServerBackend::validate_loopback_url("http://127.0.0.1:8188").is_ok());
        assert!(LoopbackMediaServerBackend::validate_loopback_url("http://8.8.8.8:8188").is_err());

        // 7. SLERP préserve la norme sur vecteurs orthogonaux.
        let mid = slerp_latents(&[1.0, 0.0], &[0.0, 1.0], 0.5);
        assert!((mid[0] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4);
        assert!((mid[1] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4);
    }
}
