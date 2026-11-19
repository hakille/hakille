//! Politiques et bornes de ressources du moteur génératif Hakille.
//!
//! Toute requête hors bornes est rejetée avant allocation de tenseurs ou appel réseau.

use crate::error::{MediaError, MediaResult};
use crate::types::{AudioGenRequest, ImageGenRequest, MediaRequest, VideoGenRequest};
use serde::{Deserialize, Serialize};

/// Plafonds de ressources et de sécurité pour la génération multimodale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaPolicy {
    pub max_prompt_bytes: usize,
    pub min_image_dim: u32,
    pub max_image_dim: u32,
    pub max_diffusion_steps: u32,
    pub max_audio_duration_secs: f32,
    pub max_video_dim: u32,
    pub max_video_frames: u32,
    pub max_video_fps: u16,
    pub max_output_bytes: u64,
    pub max_concurrent_jobs: usize,
    pub max_store_bytes: u64,
    pub max_deadline_ms: u64,
    pub allow_cloud_by_default: bool,
    pub minor_profile: bool,
}

impl Default for MediaPolicy {
    fn default() -> Self {
        Self {
            max_prompt_bytes: 4_096,
            min_image_dim: 64,
            max_image_dim: 2_048,
            max_diffusion_steps: 100,
            max_audio_duration_secs: 120.0,
            max_video_dim: 1_280,
            max_video_frames: 600,
            max_video_fps: 60,
            max_output_bytes: 512 * 1024 * 1024,
            max_concurrent_jobs: 2,
            max_store_bytes: 2 * 1024 * 1024 * 1024,
            max_deadline_ms: 900_000,
            allow_cloud_by_default: false,
            minor_profile: false,
        }
    }
}

impl MediaPolicy {
    /// Politique renforcée pour un profil mineur (Règle Constitutionnelle 7).
    pub fn for_minor() -> Self {
        Self {
            max_image_dim: 1_024,
            max_video_dim: 720,
            max_video_frames: 240,
            max_audio_duration_secs: 60.0,
            allow_cloud_by_default: false,
            minor_profile: true,
            ..Self::default()
        }
    }

    pub fn validate_request(&self, req: &MediaRequest) -> MediaResult<()> {
        match req {
            MediaRequest::Image(r) => self.validate_image(r),
            MediaRequest::Audio(r) => self.validate_audio(r),
            MediaRequest::Video(r) => self.validate_video(r),
        }
    }

    pub fn validate_prompt(&self, prompt: &str) -> MediaResult<()> {
        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            return Err(MediaError::InvalidRequest(
                "le prompt ne peut pas être vide".into(),
            ));
        }
        if trimmed.len() > self.max_prompt_bytes {
            return Err(MediaError::PolicyExceeded(format!(
                "prompt de {} octets > limite de {} octets",
                trimmed.len(),
                self.max_prompt_bytes
            )));
        }
        if trimmed.chars().any(|c| c == '\0') {
            return Err(MediaError::InvalidRequest(
                "caractère NUL interdit dans le prompt".into(),
            ));
        }
        Ok(())
    }

    pub fn validate_image(&self, req: &ImageGenRequest) -> MediaResult<()> {
        self.validate_prompt(&req.prompt)?;
        if req.width < self.min_image_dim
            || req.height < self.min_image_dim
            || req.width > self.max_image_dim
            || req.height > self.max_image_dim
        {
            return Err(MediaError::PolicyExceeded(format!(
                "dimensions {}x{} hors bornes [{}..{}]",
                req.width, req.height, self.min_image_dim, self.max_image_dim
            )));
        }
        if !req.width.is_multiple_of(8) || !req.height.is_multiple_of(8) {
            return Err(MediaError::InvalidRequest(format!(
                "dimensions {}x{} doivent être multiples de 8 (requis par les VAE latents)",
                req.width, req.height
            )));
        }
        if req.steps == 0 || req.steps > self.max_diffusion_steps {
            return Err(MediaError::PolicyExceeded(format!(
                "steps={} hors bornes [1..{}]",
                req.steps, self.max_diffusion_steps
            )));
        }
        if !req.guidance_scale.is_finite() || !(0.0..=30.0).contains(&req.guidance_scale) {
            return Err(MediaError::InvalidRequest(format!(
                "guidance_scale={} invalide (attendu dans [0, 30])",
                req.guidance_scale
            )));
        }
        self.validate_deadline(req.deadline_ms)?;
        Ok(())
    }

    pub fn validate_audio(&self, req: &AudioGenRequest) -> MediaResult<()> {
        self.validate_prompt(&req.prompt)?;
        if !req.duration_secs.is_finite()
            || req.duration_secs <= 0.05
            || req.duration_secs > self.max_audio_duration_secs
        {
            return Err(MediaError::PolicyExceeded(format!(
                "durée audio {:.2}s hors bornes (0.05..={:.1}s)",
                req.duration_secs, self.max_audio_duration_secs
            )));
        }
        if !matches!(req.sample_rate, 16_000 | 22_050 | 24_000 | 44_100 | 48_000) {
            return Err(MediaError::InvalidRequest(format!(
                "taux d'échantillonnage {} Hz non supporté (attendu 16000/22050/24000/44100/48000)",
                req.sample_rate
            )));
        }
        if !matches!(req.channels, 1 | 2) {
            return Err(MediaError::InvalidRequest(format!(
                "nombre de canaux {} invalide (attendu 1 ou 2)",
                req.channels
            )));
        }
        self.validate_deadline(req.deadline_ms)?;
        Ok(())
    }

    pub fn validate_video(&self, req: &VideoGenRequest) -> MediaResult<()> {
        self.validate_prompt(&req.prompt)?;
        if req.width < self.min_image_dim
            || req.height < self.min_image_dim
            || req.width > self.max_video_dim
            || req.height > self.max_video_dim
        {
            return Err(MediaError::PolicyExceeded(format!(
                "dimensions vidéo {}x{} hors bornes [{}..{}]",
                req.width, req.height, self.min_image_dim, self.max_video_dim
            )));
        }
        if !req.width.is_multiple_of(2) || !req.height.is_multiple_of(2) {
            return Err(MediaError::InvalidRequest(format!(
                "dimensions vidéo {}x{} doivent être paires (requis par YUV420p)",
                req.width, req.height
            )));
        }
        if req.fps == 0 || req.fps > self.max_video_fps {
            return Err(MediaError::PolicyExceeded(format!(
                "fps={} hors bornes [1..{}]",
                req.fps, self.max_video_fps
            )));
        }
        if req.num_frames == 0 || req.num_frames > self.max_video_frames {
            return Err(MediaError::PolicyExceeded(format!(
                "num_frames={} hors bornes [1..{}]",
                req.num_frames, self.max_video_frames
            )));
        }
        if let Some(audio) = &req.audio_track {
            self.validate_audio(audio)?;
        }
        self.validate_deadline(req.deadline_ms)?;
        Ok(())
    }

    fn validate_deadline(&self, deadline_ms: u64) -> MediaResult<()> {
        if deadline_ms == 0 || deadline_ms > self.max_deadline_ms {
            return Err(MediaError::PolicyExceeded(format!(
                "échéance {} ms hors bornes [1..{}]",
                deadline_ms, self.max_deadline_ms
            )));
        }
        Ok(())
    }
}
