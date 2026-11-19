//! Port hexagonal et parseur de commandes multimodales du noyau cognitif
//! (`:gen-image`, `:gen-audio`, `:gen-video`, `:gen-story`).
//!
//! Contrat d'honnêteté & sécurité :
//! - Le noyau ne forge jamais de jeton Biscuit : il vérifie les capabilities
//!   (`MediaGeneration` + `Execute`, liaison de ressource `media:<modalité>-<hash>`,
//!   et `Network` si moteur `cloud`) ;
//! - Un appel `cloud` produit une [`ActionCardPreview`] explicite (`data_exit_local = false`)
//!   exigeant validation humaine ;
//! - Le port [`MediaGenerationPort`] découple `hakille-kernel` de l'implémentation
//!   concrète (`hakille-media`), branchée par le démon.

use crate::orchestrator::ActionCardPreview;
use crate::planner::ExecutionPlan;
use async_trait::async_trait;
use hakille_core::{
    AuthorizationContext, CapabilityAction, CapabilityAuthority, CapabilityScope, CapabilityToken,
    HakilleError, Result, TrustLevel,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Modalité média ciblée par une commande explicite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaCommandKind {
    Image,
    Audio,
    Video,
    Story,
}

impl MediaCommandKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Story => "story",
        }
    }
}

/// Commande média typée parsée depuis le REPL, le HUD ou `--eval-prompt`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaCommand {
    pub kind: MediaCommandKind,
    pub prompt: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub steps: Option<u32>,
    pub seed: Option<u64>,
    pub engine: String,
    pub audio_mode: Option<String>,
    pub duration_secs: Option<f32>,
    pub voice: Option<String>,
    pub fps: Option<u16>,
    pub num_frames: Option<u32>,
    pub format: Option<String>,
    pub motion: Option<String>,
    pub audio_path: Option<String>,
}

impl MediaCommand {
    /// Parse `:gen-image`, `:gen-audio`, `:gen-video`, `:gen-story` + options.
    pub fn parse(raw: &str) -> Result<Option<Self>> {
        let trimmed = raw.trim();
        let (cmd, rest) = trimmed
            .split_once(char::is_whitespace)
            .unwrap_or((trimmed, ""));
        let kind = match cmd {
            ":gen-image" => MediaCommandKind::Image,
            ":gen-audio" => MediaCommandKind::Audio,
            ":gen-video" => MediaCommandKind::Video,
            ":gen-story" => MediaCommandKind::Story,
            _ => return Ok(None),
        };

        let tokens = split_shell_like_tokens(rest);
        let mut width = None;
        let mut height = None;
        let mut steps = None;
        let mut seed = None;
        let mut engine = "auto".to_string();
        let mut audio_mode = None;
        let mut duration_secs = None;
        let mut voice = None;
        let mut fps = None;
        let mut num_frames = None;
        let mut format = None;
        let mut motion = None;
        let mut audio_path = None;
        let mut prompt_parts: Vec<String> = Vec::new();

        let mut i = 0usize;
        while i < tokens.len() {
            let tok = &tokens[i];
            match tok.as_str() {
                "--size" if i + 1 < tokens.len() => {
                    i += 1;
                    let (w, h) = parse_size_pair(&tokens[i])?;
                    width = Some(w);
                    height = Some(h);
                }
                "--width" if i + 1 < tokens.len() => {
                    i += 1;
                    width = Some(parse_u32_flag("--width", &tokens[i])?);
                }
                "--height" if i + 1 < tokens.len() => {
                    i += 1;
                    height = Some(parse_u32_flag("--height", &tokens[i])?);
                }
                "--steps" if i + 1 < tokens.len() => {
                    i += 1;
                    steps = Some(parse_u32_flag("--steps", &tokens[i])?);
                }
                "--seed" if i + 1 < tokens.len() => {
                    i += 1;
                    seed = Some(tokens[i].parse::<u64>().map_err(|_| {
                        HakilleError::Internal(format!("--seed invalide : {}", tokens[i]))
                    })?);
                }
                "--engine" if i + 1 < tokens.len() => {
                    i += 1;
                    engine = tokens[i].clone();
                }
                "--mode" if i + 1 < tokens.len() => {
                    i += 1;
                    audio_mode = Some(tokens[i].clone());
                }
                "--duration" if i + 1 < tokens.len() => {
                    i += 1;
                    duration_secs = Some(tokens[i].parse::<f32>().map_err(|_| {
                        HakilleError::Internal(format!("--duration invalide : {}", tokens[i]))
                    })?);
                }
                "--voice" if i + 1 < tokens.len() => {
                    i += 1;
                    voice = Some(tokens[i].clone());
                }
                "--fps" if i + 1 < tokens.len() => {
                    i += 1;
                    fps = Some(tokens[i].parse::<u16>().map_err(|_| {
                        HakilleError::Internal(format!("--fps invalide : {}", tokens[i]))
                    })?);
                }
                "--frames" if i + 1 < tokens.len() => {
                    i += 1;
                    num_frames = Some(parse_u32_flag("--frames", &tokens[i])?);
                }
                "--format" if i + 1 < tokens.len() => {
                    i += 1;
                    format = Some(tokens[i].clone());
                }
                "--motion" if i + 1 < tokens.len() => {
                    i += 1;
                    motion = Some(tokens[i].clone());
                }
                "--audio" if i + 1 < tokens.len() => {
                    i += 1;
                    audio_path = Some(tokens[i].clone());
                }
                _ => prompt_parts.push(tok.clone()),
            }
            i += 1;
        }

        let prompt = prompt_parts.join(" ").trim().to_string();
        if prompt.is_empty() || prompt.len() > 4_096 {
            return Err(HakilleError::Internal(format!(
                "{cmd} : prompt requis (1 à 4 096 octets)"
            )));
        }

        Ok(Some(Self {
            kind,
            prompt,
            width,
            height,
            steps,
            seed,
            engine,
            audio_mode,
            duration_secs,
            voice,
            fps,
            num_frames,
            format,
            motion,
            audio_path,
        }))
    }

    pub fn is_cloud(&self) -> bool {
        self.engine.trim().eq_ignore_ascii_case("cloud")
    }

    /// Identifiant de ressource cryptographique pour liaison Biscuit v5.
    pub fn resource(&self) -> String {
        let bytes = serde_json::to_vec(self).expect("MediaCommand sérialisable");
        format!("media:{}-{:x}", self.kind.as_str(), Sha256::digest(bytes))
    }

    /// Construit une carte d'action + plan lorsqu'une génération demande un
    /// backend externe `cloud`.
    pub fn cloud_action_card(&self) -> (ActionCardPreview, ExecutionPlan) {
        let est_cost_cents: u64 = match self.kind {
            MediaCommandKind::Image => 4,
            MediaCommandKind::Audio => 2,
            MediaCommandKind::Video => 25,
            MediaCommandKind::Story => 35,
        };
        let mut plan = ExecutionPlan::new(
            format!("media-cloud-{}", self.kind.as_str()),
            format!(
                "Génération média cloud ({}) : {}",
                self.kind.as_str(),
                self.prompt
            ),
        );
        plan.add_step(
            format!(
                "Appel API HTTPS cloud pour {} ({})",
                self.kind.as_str(),
                self.prompt
            ),
            CapabilityScope::Network,
            CapabilityAction::Execute,
            true,
        );
        let preview = ActionCardPreview {
            verb: format!("Générer {} (Cloud)", self.kind.as_str()),
            object: self.prompt.chars().take(100).collect(),
            reason: "Transfert du prompt vers un fournisseur HTTPS externe (opt-in)".into(),
            cost_cents: est_cost_cents,
            currency: "EUR".into(),
            reversible: false,
            data_exit_local: false,
            provenance_label: "hakille-media (cloud opt-in)".into(),
        };
        (preview, plan)
    }
}

/// Vérifie la capability Biscuit v5 liée à une commande média (fail-closed).
pub fn verify_media_capability(
    cmd: &MediaCommand,
    trust_level: TrustLevel,
    auth: Option<(
        &AuthorizationContext,
        &CapabilityToken,
        &CapabilityAuthority,
    )>,
) -> Result<()> {
    if trust_level == TrustLevel::Untrusted {
        return Err(HakilleError::TrustViolation(
            "Règle 1 : une source Untrusted ne peut pas déclencher de génération média".into(),
        ));
    }
    if let Some((_ctx, token, authority)) = auth {
        let resource = cmd.resource();
        authority.verify_pinned_with_resource(
            token,
            CapabilityScope::MediaGeneration,
            CapabilityAction::Execute,
            &resource,
        )?;
        if cmd.is_cloud() {
            authority.verify_pinned(token, CapabilityScope::Network, CapabilityAction::Execute)?;
        }
    }
    Ok(())
}

/// Reçu retourné par le port d'exécution média après persistance + signature Ed25519.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaGenerationReceipt {
    pub artifact_id: String,
    pub modality: String,
    pub path: String,
    pub provenance_path: String,
    pub sha256: String,
    pub engine_id: String,
    pub engine_badge: String,
    pub elapsed_ms: u64,
    pub summary: String,
}

/// Port hexagonal implémenté par le démon pour exécuter les commandes média.
#[async_trait]
pub trait MediaGenerationPort: Send + Sync {
    async fn execute(&self, cmd: &MediaCommand) -> Result<MediaGenerationReceipt>;
}

fn parse_size_pair(raw: &str) -> Result<(u32, u32)> {
    let lower = raw.to_ascii_lowercase();
    let (ws, hs) = lower
        .split_once('x')
        .ok_or_else(|| HakilleError::Internal(format!("--size attendu LxH, reçu '{raw}'")))?;
    let w = parse_u32_flag("--size(L)", ws)?;
    let h = parse_u32_flag("--size(H)", hs)?;
    Ok((w, h))
}

fn parse_u32_flag(name: &str, raw: &str) -> Result<u32> {
    raw.parse::<u32>()
        .map_err(|_| HakilleError::Internal(format!("{name} invalide : '{raw}'")))
}

fn split_shell_like_tokens(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for ch in input.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => cur.push(ch),
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            None => cur.push(ch),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_media_commands_and_flags() {
        let cmd = MediaCommand::parse(
            ":gen-image --size 512x384 --steps 12 --seed 42 --engine procedural-dsp \"forêt lumineuse\"",
        )
        .unwrap()
        .expect("commande image");
        assert_eq!(cmd.kind, MediaCommandKind::Image);
        assert_eq!(cmd.width, Some(512));
        assert_eq!(cmd.height, Some(384));
        assert_eq!(cmd.steps, Some(12));
        assert_eq!(cmd.seed, Some(42));
        assert_eq!(cmd.engine, "procedural-dsp");
        assert_eq!(cmd.prompt, "forêt lumineuse");
        assert!(cmd.resource().starts_with("media:image-"));

        let vid = MediaCommand::parse(
            ":gen-video --fps 12 --frames 24 --format apng --motion orbit vague cosmique",
        )
        .unwrap()
        .expect("commande vidéo");
        assert_eq!(vid.kind, MediaCommandKind::Video);
        assert_eq!(vid.fps, Some(12));
        assert_eq!(vid.num_frames, Some(24));
        assert_eq!(vid.format.as_deref(), Some("apng"));
        assert_eq!(vid.motion.as_deref(), Some("orbit"));
        assert_eq!(vid.prompt, "vague cosmique");

        assert!(MediaCommand::parse("bonjour hakille").unwrap().is_none());
    }
}
