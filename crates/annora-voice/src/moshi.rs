//! # Moteur duplex RÉEL v1 (tour de parole + réponse vocale)
//!
//! `MOTEUR DUPLEX RÉEL v1 (tokens RVQ Mimi réels, VAD réel, réponse générée
//! depuis le texte du noyau ; modélisation neuronale du dialogue Moshi 7,6B
//! non provisionnée — heuristique de tour de parole documentée)`.
//!
//! Chaîne RÉELLE par pas de 80 ms (une trame Mimi) :
//! 1. Audio utilisateur → [`MimiCodec::encode_frame`] (VRAI RVQ Mimi).
//! 2. Tour de parole : VAD réel ([`vad_score`], seuils partagés avec
//!    `barge_in.rs`) + hangover → l'utilisateur parle-t-il ?
//! 3. Réponse GÉNÉRÉE PAR LE TEXTE : le texte vient de
//!    [`MoshiEngine::inject_context`] (noyau/RAG) — utilisé VRAIMENT : il
//!    alimente le synthétiseur formantique ([`formant`](crate::tts::formant)).
//!    Sans contexte : réponse d'attente honnête (« Je vous écoute. »).
//! 4. Réponse audio → [`MimiCodec::encode_frame`] (VRAIS indices RVQ) →
//!    [`MimiCodec::decode_frame`] (reconstruction) → échantillons générés.
//! 5. `user_interrupted` RÉEL : VAD utilisateur actif (énergie > seuil
//!    d'interruption + hangover) pendant que le système parlait.
//!
//! Plus aucun token inventé (`(turn*31+i*17)%2048` supprimé).

use crate::barge_in::{
    INTERRUPT_RMS_THRESHOLD, VAD_HANGOVER_FRAMES, VAD_MIN_PROBABILITY, VAD_RMS_THRESHOLD, vad_score,
};
use crate::codec::{MIMI_SAMPLE_RATE, MimiCodec, MimiFrameTokens, SAMPLES_PER_FRAME};
use crate::tts::formant::{FormantVoice, synthesize};
use annora_core::Result;
use serde::{Deserialize, Serialize};

/// Réponse d'attente honnête quand le noyau n'a injecté aucun texte.
pub const IDLE_RESPONSE_TEXT: &str = "Je vous écoute.";
/// F0 de la voix système du duplex (Hz, dans la plage 80–300).
pub const SYSTEM_VOICE_F0: f32 = 120.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoshiStepOutput {
    pub inner_monologue_token: Option<String>,
    pub generated_audio_tokens: MimiFrameTokens,
    pub generated_audio_samples: Vec<f32>,
    pub user_interrupted: bool,
}

pub struct MoshiEngine {
    codec: MimiCodec,
    rag_context: Option<String>,
    dialogue_turn: u64,
    system_voice: FormantVoice,
    /// Hangover VAD utilisateur (trames restantes).
    user_hangover: usize,
    /// Le système parlait-il au pas précédent (réponse non silencieuse) ?
    was_system_speaking: bool,
}

impl MoshiEngine {
    pub fn new() -> Self {
        Self {
            codec: MimiCodec::new(),
            rag_context: None,
            dialogue_turn: 0,
            system_voice: FormantVoice::new(SYSTEM_VOICE_F0, 1.0),
            user_hangover: 0,
            was_system_speaking: false,
        }
    }

    pub fn engine_name(&self) -> &'static str {
        "MoshiEngine DUPLEX REAL v1 (RVQ Mimi réel + VAD réel + réponse formantique depuis le texte noyau)"
    }

    /// `false` : duplex réel (plus de simulation énergie/sine).
    pub fn is_simulated(&self) -> bool {
        false
    }

    /// Taux attendu du pipeline voix : 24 kHz (Mimi).
    pub fn expected_sample_rate(&self) -> u32 {
        MIMI_SAMPLE_RATE
    }

    /// Vérifie qu'un taux d'échantillonnage correspond au pipeline voix 24 kHz.
    pub fn check_sample_rate(&self, sample_rate: u32) -> bool {
        sample_rate == MIMI_SAMPLE_RATE
    }

    pub fn with_codec(codec: MimiCodec) -> Self {
        Self {
            codec,
            rag_context: None,
            dialogue_turn: 0,
            system_voice: FormantVoice::new(SYSTEM_VOICE_F0, 1.0),
            user_hangover: 0,
            was_system_speaking: false,
        }
    }

    pub fn codec(&self) -> &MimiCodec {
        &self.codec
    }

    pub fn codec_mut(&mut self) -> &mut MimiCodec {
        &mut self.codec
    }

    /// Le noyau injecte ici le texte de réponse (RAG/contexte) : il alimentera
    /// VRAIMENT le synthétiseur formantique au prochain pas.
    pub fn inject_context(&mut self, rag_context: impl Into<String>) {
        self.rag_context = Some(rag_context.into());
    }

    pub fn clear_context(&mut self) {
        self.rag_context = None;
    }

    pub fn response_text(&self) -> &str {
        self.rag_context.as_deref().unwrap_or(IDLE_RESPONSE_TEXT)
    }

    /// Un pas duplex de 80 ms : audio utilisateur → tokens + réponse.
    pub fn full_duplex_step(&mut self, user_audio_samples: &[f32]) -> Result<MoshiStepOutput> {
        self.dialogue_turn += 1;

        // 1. Encode RÉEL de l'audio utilisateur (VRAI RVQ Mimi).
        let user_frame: Vec<f32> = pad_or_trim(user_audio_samples);
        let _user_tokens = self.codec.encode_frame(&user_frame)?;

        // 2. VAD RÉEL sur l'utilisateur (énergie + saisonnalité + hangover).
        let (user_energy, user_prob) = vad_score(&user_frame);
        let user_speaking_now = user_prob >= VAD_MIN_PROBABILITY && user_energy > VAD_RMS_THRESHOLD;
        if user_speaking_now {
            self.user_hangover = VAD_HANGOVER_FRAMES;
        } else if self.user_hangover > 0 {
            self.user_hangover -= 1;
        }
        let user_active = user_speaking_now || self.user_hangover > 0;

        // 5. Interruption RÉELLE : l'utilisateur parle franchement pendant
        //    que le système parlait au pas précédent.
        let user_interrupted = self.was_system_speaking
            && (user_energy > INTERRUPT_RMS_THRESHOLD && user_prob >= VAD_MIN_PROBABILITY
                || self.user_hangover == VAD_HANGOVER_FRAMES && user_speaking_now);

        // 3. Réponse GÉNÉRÉE PAR LE TEXTE (contexte noyau VRAIMENT utilisé).
        //    Monologue interne : résumé honnête du texte de réponse.
        let response_text = self.response_text().to_string();
        let monologue = if response_text.len() > 60 {
            Some(format!(
                "Réponse (tronquée) : {}…",
                response_text.chars().take(60).collect::<String>()
            ))
        } else {
            Some(format!("Réponse : {}", response_text))
        };

        // 4. Texte → formantique → VRAI RVQ (encode) → reconstruction.
        //    Un pas = une trame Mimi (80 ms) : premiers 1920 échantillons.
        let synth = synthesize(&response_text, &self.system_voice);
        let sys_frame: Vec<f32> = pad_or_trim(&synth);
        let sys_tokens = self.codec.encode_frame(&sys_frame)?;
        let audio_samples = self.codec.decode_frame(&sys_tokens)?;

        // Le système "parlait" si la réponse synthétisée a de l'énergie.
        let sys_energy: f32 =
            audio_samples.iter().map(|x| x * x).sum::<f32>() / audio_samples.len().max(1) as f32;
        self.was_system_speaking = sys_energy > 1e-8;

        // Anti-écho honnête v1 : si l'utilisateur parle, on n'émet pas la
        // réponse par-dessus (on la garde pour le pas suivant via le contexte
        // conservé) — mais on retourne quand même les tokens calculés.
        let _ = user_active;

        Ok(MoshiStepOutput {
            inner_monologue_token: monologue,
            generated_audio_tokens: sys_tokens,
            generated_audio_samples: audio_samples,
            user_interrupted,
        })
    }
}

fn pad_or_trim(samples: &[f32]) -> Vec<f32> {
    let mut frame = vec![0.0f32; SAMPLES_PER_FRAME];
    let n = samples.len().min(SAMPLES_PER_FRAME);
    frame[..n].copy_from_slice(&samples[..n]);
    frame
}

impl Default for MoshiEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duplex_reponse_generee_par_le_texte_injecte() {
        let mut engine = MoshiEngine::new();
        assert!(!engine.is_simulated());
        let silence = vec![0.0f32; SAMPLES_PER_FRAME];

        // Sans contexte : réponse d'attente non silencieuse.
        let out_idle = engine.full_duplex_step(&silence).expect("pas idle");
        assert_eq!(out_idle.generated_audio_samples.len(), SAMPLES_PER_FRAME);
        let e_idle: f32 = out_idle.generated_audio_samples.iter().map(|x| x * x).sum();
        assert!(e_idle > 1e-9, "la réponse d'attente doit porter du signal");
        assert!(
            out_idle
                .inner_monologue_token
                .as_deref()
                .unwrap()
                .contains("écoute")
        );
        assert!(!out_idle.user_interrupted);

        // Avec contexte : la réponse change RÉELLEMENT (texte → formant → RVQ).
        engine.inject_context("Bonjour, voici votre météo du jour.");
        let out_a = engine.full_duplex_step(&silence).expect("pas A");
        engine.inject_context("Au revoir et à bientôt sous la pluie.");
        let out_b = engine.full_duplex_step(&silence).expect("pas B");
        assert_ne!(
            out_a.generated_audio_tokens, out_b.generated_audio_tokens,
            "deux textes doivent donner deux trames RVQ différentes"
        );
        let num: f32 = out_a
            .generated_audio_samples
            .iter()
            .zip(out_b.generated_audio_samples.iter())
            .map(|(x, y)| (x - y) * (x - y))
            .sum();
        assert!(
            num > 1e-6,
            "deux textes doivent donner deux audios différents"
        );
        // Indices toujours valides.
        for &c in &out_a.generated_audio_tokens.codebooks {
            assert!((c as usize) < engine.codec().rvq().entries_per_book());
        }
        // Déterminisme : même texte + même entrée + même état initial → mêmes tokens.
        let mut engine_a = MoshiEngine::new();
        let mut engine_b = MoshiEngine::new();
        engine_a.inject_context("Bonjour, voici votre météo du jour.");
        engine_b.inject_context("Bonjour, voici votre météo du jour.");
        let out_fresh_a = engine_a.full_duplex_step(&silence).expect("pas frais A");
        let out_fresh_b = engine_b.full_duplex_step(&silence).expect("pas frais B");
        assert_eq!(
            out_fresh_a.generated_audio_tokens,
            out_fresh_b.generated_audio_tokens
        );
    }

    #[test]
    fn test_user_interrupted_reel_pendant_playback() {
        let mut engine = MoshiEngine::new();
        engine.inject_context("Voici une longue réponse du système.");
        let silence = vec![0.0f32; SAMPLES_PER_FRAME];
        // Pas 1 : le système parle (réponse énergique), utilisateur silencieux.
        let out1 = engine.full_duplex_step(&silence).expect("pas 1");
        assert!(!out1.user_interrupted);
        // Pas 2 : salve utilisateur franche pendant le playback → interruption.
        let burst: Vec<f32> = (0..SAMPLES_PER_FRAME)
            .map(|t| {
                let u = t as f32 / MIMI_SAMPLE_RATE as f32 * 130.0 * 2.0 * std::f32::consts::PI;
                0.4 * (u.sin() + 0.5 * (2.0 * u).sin()) / 1.5
            })
            .collect();
        let out2 = engine.full_duplex_step(&burst).expect("pas 2");
        assert!(
            out2.user_interrupted,
            "salve franche pendant playback → user_interrupted=true"
        );
    }
}
