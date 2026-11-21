//! Adaptateur cloud HTTPS opt-in (`cloud`).
//!
//! Contrat de sécurité & d'honnêteté :
//! - Désactivé par défaut (`EngineAvailability::Unconfigured`) tant que
//!   `HAKILLE_MEDIA_CLOUD_BASE_URL` (HTTPS obligatoire) et
//!   `HAKILLE_MEDIA_CLOUD_API_KEY` ne sont pas définis ;
//! - Validation anti-SSRF stricte : rejette toute adresse privée, loopback,
//!   link-local ou schéma non HTTPS ;
//! - TLS vérifié (jamais de contournement TLS) ;
//! - Exige en amont l'autorisation [`crate::guard::MediaConstitutionGuard`]
//!   (Biscuit `MediaGeneration` + `Network` + approbation explicite + Règles 7/8).

use crate::audio::mastering::read_wav_file;
use crate::audio::{AudioGenerationOutput, AudioGeneratorBackend};
use crate::error::{MediaError, MediaResult};
use crate::image::encode::RgbaImageBuf;
use crate::image::{ImageGenerationOutput, ImageGeneratorBackend};
use crate::types::{
    AudioGenRequest, EngineAvailability, EngineDescriptor, EngineKind, GenerationControl,
    ImageGenRequest, MediaDataExit, MediaModality, ProgressCallback, derive_seed,
};
use async_trait::async_trait;
use base64::Engine as _;
use std::net::IpAddr;
use std::time::Duration;

const MAX_CLOUD_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone)]
pub struct CloudMediaBackend {
    base_url: Option<String>,
    api_key: Option<String>,
    provider_name: String,
    model_name: String,
}

impl std::fmt::Debug for CloudMediaBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudMediaBackend")
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<masqué>"))
            .field("provider_name", &self.provider_name)
            .field("model_name", &self.model_name)
            .finish()
    }
}

impl CloudMediaBackend {
    pub fn from_env() -> Self {
        let base_url = std::env::var("HAKILLE_MEDIA_CLOUD_BASE_URL")
            .ok()
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty());
        let api_key = std::env::var("HAKILLE_MEDIA_CLOUD_API_KEY")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let provider_name = std::env::var("HAKILLE_MEDIA_CLOUD_PROVIDER")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "custom-https".into());
        let model_name = std::env::var("HAKILLE_MEDIA_CLOUD_MODEL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "gpt-image-1".into());
        Self {
            base_url,
            api_key,
            provider_name,
            model_name,
        }
    }

    /// Validation anti-SSRF : HTTPS strict + blocage des IP privées/locales.
    pub fn validate_cloud_url(raw: &str) -> MediaResult<reqwest::Url> {
        let parsed = reqwest::Url::parse(raw)
            .map_err(|e| MediaError::InvalidRequest(format!("URL cloud invalide : {e}")))?;
        if parsed.scheme() != "https" {
            return Err(MediaError::InvalidRequest(
                "un fournisseur média cloud DOIT utiliser HTTPS (TLS vérifié)".into(),
            ));
        }
        if !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(MediaError::InvalidRequest(
                "URL cloud : identifiants, query et fragment interdits".into(),
            ));
        }
        let host = parsed
            .host_str()
            .ok_or_else(|| MediaError::InvalidRequest("hôte absent dans l'URL cloud".into()))?;
        let lower = host.to_ascii_lowercase();
        if lower == "localhost" || lower.ends_with(".local") || lower.ends_with(".internal") {
            return Err(MediaError::ConstitutionBlocked(format!(
                "SSRF bloqué : hôte interne '{host}' interdit pour le backend cloud"
            )));
        }
        let trimmed = lower.trim_start_matches('[').trim_end_matches(']');
        if let Ok(ip) = trimmed.parse::<IpAddr>()
            && is_private_or_local_ip(ip)
        {
            return Err(MediaError::ConstitutionBlocked(format!(
                "SSRF bloqué : adresse IP privée/locale '{ip}' interdite pour le backend cloud"
            )));
        }
        Ok(parsed)
    }

    pub fn data_exit(&self) -> MediaDataExit {
        MediaDataExit::Cloud {
            provider: self.provider_name.clone(),
            endpoint: self.base_url.clone().unwrap_or_default(),
        }
    }

    fn probe(&self) -> EngineAvailability {
        match (&self.base_url, &self.api_key) {
            (Some(url), Some(_)) => match Self::validate_cloud_url(url) {
                Ok(_) => EngineAvailability::Ready {
                    device: format!("Cloud HTTPS ({})", self.provider_name),
                    model_sha256: None,
                },
                Err(e) => EngineAvailability::Unavailable {
                    reason: e.to_string(),
                },
            },
            _ => EngineAvailability::Unconfigured {
                env_hint: "Définir HAKILLE_MEDIA_CLOUD_BASE_URL (https://...) et HAKILLE_MEDIA_CLOUD_API_KEY (+ approbation ActionCard)".into(),
            },
        }
    }

    async fn build_client(
        &self,
        timeout_ms: u64,
    ) -> MediaResult<(reqwest::Client, reqwest::Url, &str)> {
        let raw_url = self
            .base_url
            .as_deref()
            .ok_or_else(|| MediaError::BackendUnavailable {
                engine_id: "cloud".into(),
                reason: "HAKILLE_MEDIA_CLOUD_BASE_URL non défini".into(),
            })?;
        let api_key = self
            .api_key
            .as_deref()
            .ok_or_else(|| MediaError::BackendUnavailable {
                engine_id: "cloud".into(),
                reason: "HAKILLE_MEDIA_CLOUD_API_KEY non défini".into(),
            })?;
        let url = Self::validate_cloud_url(raw_url)?;
        let host = url
            .host_str()
            .ok_or_else(|| MediaError::InvalidRequest("hôte absent dans l'URL cloud".into()))?;
        let port = url.port_or_known_default().unwrap_or(443);

        // Résolution DNS explicite + blocage des IP privées/locales + épinglage
        // des adresses validées (bloque DNS rebinding et domaines pointant vers
        // 127.0.0.1 / 169.254.169.254).
        let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host, port))
            .await
            .map_err(|e| MediaError::Network(format!("résolution DNS cloud ({host}) : {e}")))?
            .collect();
        if addrs.is_empty() {
            return Err(MediaError::Network(format!(
                "aucune adresse IP résolue pour l'hôte cloud '{host}'"
            )));
        }
        for addr in &addrs {
            if is_private_or_local_ip(addr.ip()) {
                return Err(MediaError::ConstitutionBlocked(format!(
                    "SSRF bloqué : '{host}' résout vers une IP privée/locale ({})",
                    addr.ip()
                )));
            }
        }

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .resolve_to_addrs(host, &addrs)
            .timeout(Duration::from_millis(timeout_ms.max(10_000)))
            .build()
            .map_err(|e| MediaError::Network(e.to_string()))?;
        Ok((client, url, api_key))
    }
}

pub(crate) async fn read_bounded_body(
    mut resp: reqwest::Response,
    max_bytes: usize,
    label: &str,
) -> MediaResult<Vec<u8>> {
    if let Some(len) = resp.content_length()
        && len > max_bytes as u64
    {
        return Err(MediaError::PolicyExceeded(format!(
            "{label} au-delà de la limite de sécurité ({len} > {max_bytes} octets)"
        )));
    }
    let mut out = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| MediaError::Network(e.to_string()))?
    {
        if out.len().saturating_add(chunk.len()) > max_bytes {
            return Err(MediaError::PolicyExceeded(format!(
                "{label} au-delà de la limite de sécurité ({max_bytes} octets)"
            )));
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

fn is_private_or_local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.is_multicast()
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0b1100_0000) == 64)
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_private_or_local_ip(IpAddr::V4(mapped));
            }
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

#[async_trait]
impl ImageGeneratorBackend for CloudMediaBackend {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            id: "cloud".into(),
            name: format!("API Média Cloud HTTPS ({})", self.provider_name),
            modality: MediaModality::Image,
            kind: EngineKind::NeuralCloudApi,
            data_exit: self.data_exit(),
            availability: self.probe(),
            estimated_ram_mb: 0,
            estimated_vram_mb: 0,
            capabilities: vec![
                "text_to_image".into(),
                "requires_network_biscuit".into(),
                "requires_action_card".into(),
            ],
            description: "Fournisseur HTTPS externe opt-in (TLS vérifié, anti-SSRF, requiert Biscuit Network + MediaGeneration + ActionCard).".into(),
        }
    }

    async fn generate(
        &self,
        req: &ImageGenRequest,
        control: &GenerationControl,
        _progress: Option<ProgressCallback>,
    ) -> MediaResult<ImageGenerationOutput> {
        control.check()?;
        let (client, base, api_key) = self.build_client(req.deadline_ms).await?;
        let url = format!(
            "{}/v1/images/generations",
            base.as_str().trim_end_matches('/')
        );
        let seed = derive_seed(&req.prompt, req.seed);
        let body = serde_json::json!({
            "model": self.model_name,
            "prompt": req.prompt,
            "size": format!("{}x{}", req.width, req.height),
            "response_format": "b64_json",
        });

        let resp = client
            .post(&url)
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| MediaError::Network(format!("requête cloud image : {e}")))?;
        if !resp.status().is_success() {
            return Err(MediaError::Network(format!(
                "fournisseur cloud image HTTP {}",
                resp.status()
            )));
        }
        let raw = read_bounded_body(resp, MAX_CLOUD_RESPONSE_BYTES, "réponse cloud image").await?;
        let parsed: serde_json::Value = serde_json::from_slice(&raw)?;
        let b64 =
            parsed["data"][0]["b64_json"]
                .as_str()
                .ok_or_else(|| MediaError::InferenceFailed {
                    engine_id: "cloud".into(),
                    reason: "réponse cloud dépourvue de data[0].b64_json".into(),
                })?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| MediaError::Encoding(e.to_string()))?;
        let decoded = image::load_from_memory(&bytes)
            .map_err(|e| MediaError::Encoding(e.to_string()))?
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
            engine_id: "cloud".into(),
            engine_kind: EngineKind::NeuralCloudApi,
            data_exit: self.data_exit(),
            model_id: Some(self.model_name.clone()),
            model_sha256: None,
        })
    }
}

#[async_trait]
impl AudioGeneratorBackend for CloudMediaBackend {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            id: "cloud".into(),
            name: format!("API Audio Cloud HTTPS ({})", self.provider_name),
            modality: MediaModality::Audio,
            kind: EngineKind::NeuralCloudApi,
            data_exit: self.data_exit(),
            availability: self.probe(),
            estimated_ram_mb: 0,
            estimated_vram_mb: 0,
            capabilities: vec![
                "speech".into(),
                "requires_network_biscuit".into(),
                "requires_action_card".into(),
            ],
            description: "Synthèse audio distante HTTPS opt-in (anti-SSRF, requiert Biscuit Network + ActionCard).".into(),
        }
    }

    async fn generate(
        &self,
        req: &AudioGenRequest,
        control: &GenerationControl,
        _progress: Option<ProgressCallback>,
    ) -> MediaResult<AudioGenerationOutput> {
        control.check()?;
        let (client, base, api_key) = self.build_client(req.deadline_ms).await?;
        let url = format!("{}/v1/audio/speech", base.as_str().trim_end_matches('/'));
        let body = serde_json::json!({
            "model": self.model_name,
            "input": req.prompt,
            "response_format": "wav",
        });
        let resp = client
            .post(&url)
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| MediaError::Network(format!("requête cloud audio : {e}")))?;
        if !resp.status().is_success() {
            return Err(MediaError::Network(format!(
                "fournisseur cloud audio HTTP {}",
                resp.status()
            )));
        }
        let wav_bytes =
            read_bounded_body(resp, MAX_CLOUD_RESPONSE_BYTES, "réponse cloud audio").await?;
        let tmp = tempfile::NamedTempFile::new()?;
        std::fs::write(tmp.path(), &wav_bytes)?;
        let buffer = read_wav_file(tmp.path())?;
        let preview = buffer.render_waveform_preview(200, 96);
        let seed = derive_seed(&req.prompt, req.seed);

        Ok(AudioGenerationOutput {
            wav_bytes,
            buffer,
            preview,
            visemes_json: None,
            seed,
            engine_id: "cloud".into(),
            engine_kind: EngineKind::NeuralCloudApi,
            data_exit: self.data_exit(),
            model_id: Some(self.model_name.clone()),
            model_sha256: None,
        })
    }
}
