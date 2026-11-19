//! Adaptateur opt-in vers un serveur local sur boucle locale (`127.0.0.1` / `[::1]`).
//!
//! Permet de brancher un moteur GPU local dédié (ComfyUI, sd.cpp-server,
//! Kokoro-FastAPI, Wan/LTX local) sans jamais quitter la machine :
//! - l'URL configurée via `HAKILLE_MEDIA_LOOPBACK_URL` DOIT être une adresse IP
//!   littérale loopback (`127.0.0.1` ou `[::1]`), schéma `http://` ;
//! - zéro proxy système (`no_proxy()`), zéro redirection HTTP (`redirect::Policy::none()`) ;
//! - sans configuration, rapporte honnêtement [`EngineAvailability::Unconfigured`].

use crate::audio::mastering::{AudioBuffer, read_wav_file};
use crate::audio::{AudioGenerationOutput, AudioGeneratorBackend};
use crate::error::{MediaError, MediaResult};
use crate::image::encode::RgbaImageBuf;
use crate::image::{ImageGenerationOutput, ImageGeneratorBackend};
use crate::types::{
    AudioGenRequest, EngineAvailability, EngineDescriptor, EngineKind, GenerationControl,
    ImageGenRequest, MediaDataExit, MediaModality, ProgressCallback, VideoContainer,
    VideoGenRequest, derive_seed,
};
use crate::video::{VideoGenerationOutput, VideoGeneratorBackend};
use async_trait::async_trait;
use base64::Engine as _;
use std::net::IpAddr;
use std::path::Path;
use std::time::Duration;

const MAX_LOOPBACK_RESPONSE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct LoopbackMediaServerBackend {
    endpoint: Option<String>,
}

impl LoopbackMediaServerBackend {
    pub fn from_env() -> Self {
        let endpoint = std::env::var("HAKILLE_MEDIA_LOOPBACK_URL")
            .ok()
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty());
        Self { endpoint }
    }

    pub fn new(endpoint: Option<String>) -> Self {
        Self {
            endpoint: endpoint
                .map(|s| s.trim().trim_end_matches('/').to_string())
                .filter(|s| !s.is_empty()),
        }
    }

    /// Valide strictement qu'une URL pointe vers `127.0.0.1` ou `[::1]`.
    pub fn validate_loopback_url(url: &str) -> MediaResult<reqwest::Url> {
        let parsed = reqwest::Url::parse(url)
            .map_err(|e| MediaError::InvalidRequest(format!("URL loopback invalide : {e}")))?;
        if parsed.scheme() != "http" {
            return Err(MediaError::InvalidRequest(
                "le serveur loopback média doit utiliser http:// sur 127.0.0.1 ou [::1]".into(),
            ));
        }
        let host = parsed
            .host_str()
            .ok_or_else(|| MediaError::InvalidRequest("hôte absent dans l'URL loopback".into()))?;
        let trimmed = host.trim_start_matches('[').trim_end_matches(']');
        let ip: IpAddr = trimmed.parse().map_err(|_| {
            MediaError::InvalidRequest(format!(
                "hôte '{host}' refusé : une IP littérale loopback (127.0.0.1 ou ::1) est exigée"
            ))
        })?;
        if !ip.is_loopback() {
            return Err(MediaError::InvalidRequest(format!(
                "adresse '{ip}' refusée : seul le loopback local est autorisé"
            )));
        }
        Ok(parsed)
    }

    fn build_client(&self, timeout_ms: u64) -> MediaResult<(reqwest::Client, reqwest::Url)> {
        let raw = self
            .endpoint
            .as_deref()
            .ok_or_else(|| MediaError::BackendUnavailable {
                engine_id: "loopback".into(),
                reason: "définir HAKILLE_MEDIA_LOOPBACK_URL=http://127.0.0.1:<port>".into(),
            })?;
        let base = Self::validate_loopback_url(raw)?;
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_millis(timeout_ms.max(5_000)))
            .build()
            .map_err(|e| MediaError::Network(e.to_string()))?;
        Ok((client, base))
    }

    fn probe(&self) -> (MediaDataExit, EngineAvailability) {
        match &self.endpoint {
            None => (
                MediaDataExit::Loopback {
                    endpoint: String::new(),
                },
                EngineAvailability::Unconfigured {
                    env_hint: "Définir HAKILLE_MEDIA_LOOPBACK_URL=http://127.0.0.1:<port>".into(),
                },
            ),
            Some(url) => match Self::validate_loopback_url(url) {
                Ok(_) => (
                    MediaDataExit::Loopback {
                        endpoint: url.clone(),
                    },
                    EngineAvailability::Ready {
                        device: format!("Loopback local ({url})"),
                        model_sha256: None,
                    },
                ),
                Err(e) => (
                    MediaDataExit::Loopback {
                        endpoint: url.clone(),
                    },
                    EngineAvailability::Unavailable {
                        reason: e.to_string(),
                    },
                ),
            },
        }
    }
}

#[async_trait]
impl ImageGeneratorBackend for LoopbackMediaServerBackend {
    fn descriptor(&self) -> EngineDescriptor {
        let (data_exit, availability) = self.probe();
        EngineDescriptor {
            id: "loopback".into(),
            name: "Serveur Neural Loopback (127.0.0.1)".into(),
            modality: MediaModality::Image,
            kind: EngineKind::NeuralLoopbackServer,
            data_exit,
            availability,
            estimated_ram_mb: 0,
            estimated_vram_mb: 0,
            capabilities: vec!["text_to_image".into(), "openai_compatible".into()],
            description: "Pont HTTP strictement loopback (127.0.0.1/[::1]) vers un moteur local GPU (sd.cpp / ComfyUI / API compatible OpenAI).".into(),
        }
    }

    async fn generate(
        &self,
        req: &ImageGenRequest,
        control: &GenerationControl,
        _progress: Option<ProgressCallback>,
    ) -> MediaResult<ImageGenerationOutput> {
        control.check()?;
        let (client, base) = self.build_client(req.deadline_ms)?;
        let url = format!(
            "{}/v1/images/generations",
            base.as_str().trim_end_matches('/')
        );
        let seed = derive_seed(&req.prompt, req.seed);
        let body = serde_json::json!({
            "prompt": req.prompt,
            "size": format!("{}x{}", req.width, req.height),
            "steps": req.steps,
            "seed": seed,
            "response_format": "b64_json",
        });

        let resp = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| MediaError::Network(format!("appel loopback image : {e}")))?;
        if !resp.status().is_success() {
            return Err(MediaError::Network(format!(
                "serveur loopback image HTTP {}",
                resp.status()
            )));
        }
        let raw = resp
            .bytes()
            .await
            .map_err(|e| MediaError::Network(e.to_string()))?;
        if raw.len() > MAX_LOOPBACK_RESPONSE_BYTES {
            return Err(MediaError::PolicyExceeded(
                "réponse loopback image trop volumineuse".into(),
            ));
        }
        let parsed: serde_json::Value = serde_json::from_slice(&raw)?;
        let b64 =
            parsed["data"][0]["b64_json"]
                .as_str()
                .ok_or_else(|| MediaError::InferenceFailed {
                    engine_id: "loopback".into(),
                    reason: "champ data[0].b64_json absent de la réponse loopback".into(),
                })?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| MediaError::Encoding(format!("base64 loopback : {e}")))?;
        let decoded = image::load_from_memory(&bytes)
            .map_err(|e| MediaError::Encoding(format!("décodage image loopback : {e}")))?
            .to_rgba8();
        let (w, h) = decoded.dimensions();
        let rgba = RgbaImageBuf::new(w, h, decoded.into_raw())?;
        let preview = rgba.make_preview(200, 120);
        let encoded_bytes = rgba.encode(req.format)?;

        Ok(ImageGenerationOutput {
            encoded_bytes,
            rgba,
            preview,
            seed,
            engine_id: "loopback".into(),
            engine_kind: EngineKind::NeuralLoopbackServer,
            data_exit: MediaDataExit::Loopback {
                endpoint: base.to_string(),
            },
            model_id: Some("loopback-image".into()),
            model_sha256: None,
        })
    }
}

#[async_trait]
impl AudioGeneratorBackend for LoopbackMediaServerBackend {
    fn descriptor(&self) -> EngineDescriptor {
        let (data_exit, availability) = self.probe();
        EngineDescriptor {
            id: "loopback".into(),
            name: "Serveur Audio Loopback (127.0.0.1)".into(),
            modality: MediaModality::Audio,
            kind: EngineKind::NeuralLoopbackServer,
            data_exit,
            availability,
            estimated_ram_mb: 0,
            estimated_vram_mb: 0,
            capabilities: vec!["speech".into(), "openai_compatible".into()],
            description: "Pont HTTP strictement loopback vers un serveur TTS/Audio local (Kokoro/Chatterbox/API OpenAI).".into(),
        }
    }

    async fn generate(
        &self,
        req: &AudioGenRequest,
        control: &GenerationControl,
        _progress: Option<ProgressCallback>,
    ) -> MediaResult<AudioGenerationOutput> {
        control.check()?;
        let (client, base) = self.build_client(req.deadline_ms)?;
        let url = format!("{}/v1/audio/speech", base.as_str().trim_end_matches('/'));
        let body = serde_json::json!({
            "input": req.prompt,
            "response_format": "wav",
        });
        let resp = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| MediaError::Network(format!("appel loopback audio : {e}")))?;
        if !resp.status().is_success() {
            return Err(MediaError::Network(format!(
                "serveur loopback audio HTTP {}",
                resp.status()
            )));
        }
        let wav_bytes = resp
            .bytes()
            .await
            .map_err(|e| MediaError::Network(e.to_string()))?
            .to_vec();
        if wav_bytes.len() > MAX_LOOPBACK_RESPONSE_BYTES {
            return Err(MediaError::PolicyExceeded(
                "réponse loopback audio trop volumineuse".into(),
            ));
        }
        let tmp = tempfile::NamedTempFile::new()?;
        std::fs::write(tmp.path(), &wav_bytes)?;
        let buffer: AudioBuffer = read_wav_file(tmp.path())?;
        let preview = buffer.render_waveform_preview(200, 96);
        let seed = derive_seed(&req.prompt, req.seed);

        Ok(AudioGenerationOutput {
            wav_bytes,
            buffer,
            preview,
            visemes_json: None,
            seed,
            engine_id: "loopback".into(),
            engine_kind: EngineKind::NeuralLoopbackServer,
            data_exit: MediaDataExit::Loopback {
                endpoint: base.to_string(),
            },
            model_id: Some("loopback-audio".into()),
            model_sha256: None,
        })
    }
}

#[async_trait]
impl VideoGeneratorBackend for LoopbackMediaServerBackend {
    fn descriptor(&self) -> EngineDescriptor {
        let (data_exit, availability) = self.probe();
        EngineDescriptor {
            id: "loopback".into(),
            name: "Serveur Vidéo Loopback (127.0.0.1)".into(),
            modality: MediaModality::Video,
            kind: EngineKind::NeuralLoopbackServer,
            data_exit,
            availability,
            estimated_ram_mb: 0,
            estimated_vram_mb: 0,
            capabilities: vec!["text_to_video".into()],
            description: "Pont HTTP strictement loopback vers un moteur vidéo local GPU.".into(),
        }
    }

    async fn generate(
        &self,
        req: &VideoGenRequest,
        _audio_wav_path: Option<&Path>,
        control: &GenerationControl,
        _progress: Option<ProgressCallback>,
    ) -> MediaResult<VideoGenerationOutput> {
        control.check()?;
        let (client, base) = self.build_client(req.deadline_ms)?;
        let url = format!(
            "{}/v1/videos/generations",
            base.as_str().trim_end_matches('/')
        );
        let seed = derive_seed(&req.prompt, req.seed);
        let body = serde_json::json!({
            "prompt": req.prompt,
            "width": req.width,
            "height": req.height,
            "fps": req.fps,
            "num_frames": req.num_frames,
            "seed": seed,
            "format": req.container.extension(),
        });
        let resp = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| MediaError::Network(format!("appel loopback vidéo : {e}")))?;
        if !resp.status().is_success() {
            return Err(MediaError::Network(format!(
                "serveur loopback vidéo HTTP {}",
                resp.status()
            )));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| MediaError::Network(e.to_string()))?
            .to_vec();
        if bytes.len() > MAX_LOOPBACK_RESPONSE_BYTES {
            return Err(MediaError::PolicyExceeded(
                "réponse loopback vidéo trop volumineuse".into(),
            ));
        }
        Ok(VideoGenerationOutput {
            encoded_bytes: bytes,
            container: VideoContainer::Mp4H264,
            preview: crate::types::PreviewImage {
                width: 0,
                height: 0,
                rgba: Vec::new(),
            },
            seed,
            engine_id: "loopback".into(),
            engine_kind: EngineKind::NeuralLoopbackServer,
            data_exit: MediaDataExit::Loopback {
                endpoint: base.to_string(),
            },
            model_id: Some("loopback-video".into()),
            model_sha256: None,
        })
    }
}
