//! Activation explicite à partir d'une transcription ASR, jamais de l'énergie
//! audio. Ce contrôleur ne vérifie aucune identité et n'accorde aucune
//! autorisation d'action. L'ASR peut se tromper : les contrôles métier restent
//! nécessaires après l'activation.
//!
//! L'appelant fournit un temps monotone en millisecondes (par exemple depuis
//! `Instant::elapsed`) au moment du traitement. `on_transcript_captured` ajoute
//! un horodatage de capture pour rejeter les résultats ASR acquis avant une
//! annulation, un traitement occupé ou la fin du cooldown.
//! Les alias sont des mots explicites; aucune correspondance approximative.

/// Paramètres de l'activation, indépendants du moteur ASR et de sa langue.
#[derive(Debug, Clone)]
pub struct WakeActivationConfig {
    /// Alias explicites, un mot chacun. Accents latins et casse normalisés.
    /// Les alias dans d'autres écritures doivent être fournis explicitement.
    pub aliases: Vec<String>,
    /// Vocatifs permis avant le nom (un mot chacun), par exemple « Dis Hakille ».
    pub vocative_prefixes: Vec<String>,
    /// Durée d'attente d'une commande après le seul mot d'activation.
    pub armed_timeout_ms: u64,
    /// Délai après la fin d'une commande avant une nouvelle activation.
    pub cooldown_ms: u64,
    /// Faux par défaut : le premier mot (ou celui après un vocatif) doit être un alias. Vrai permet une
    /// invocation ailleurs, mais accepte aussi des mentions conversationnelles.
    pub allow_wake_anywhere: bool,
    /// Limite en octets UTF-8, vérifiée avant toute allocation de parsing.
    pub max_transcript_bytes: usize,
}

impl Default for WakeActivationConfig {
    fn default() -> Self {
        Self {
            aliases: vec!["Hakille".into(), "Hakil".into(), "Hakill".into()],
            vocative_prefixes: [
                "dis", "hey", "salut", "bonjour", "ok", "okay", "hello", "hola",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            armed_timeout_ms: 10_000,
            cooldown_ms: 1_000,
            allow_wake_anywhere: false,
            max_transcript_bytes: 8_192,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("configuration d'activation invalide : {0}")]
pub struct WakeActivationError(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationState {
    Idle,
    Armed { expires_at_ms: u64 },
    Busy,
    Cooldown { until_ms: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationIgnoreReason {
    Empty,
    NoWakeWord,
    Busy,
    Cooldown,
    StaleTimestamp,
    TextTooLong,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationOutcome {
    Ignored {
        reason: ActivationIgnoreReason,
    },
    Armed {
        expires_at_ms: u64,
    },
    /// Le contrôleur passe immédiatement à Busy : pas de deuxième commande
    /// avant `finish_command` ou `reset`, même si l'appelant traite en async.
    Command {
        text: String,
        language: String,
    },
}

pub struct WakeActivation {
    config: WakeActivationConfig,
    aliases: Vec<String>,
    vocative_prefixes: Vec<String>,
    state: ActivationState,
    last_time_ms: Option<u64>,
    minimum_capture_ms: u64,
}

impl WakeActivation {
    pub fn new(config: WakeActivationConfig) -> Result<Self, WakeActivationError> {
        if config.aliases.is_empty()
            || config.aliases.len() > 64
            || config.vocative_prefixes.len() > 64
        {
            return Err(WakeActivationError("1 à 64 alias requis".into()));
        }
        if !(1..=120_000).contains(&config.armed_timeout_ms)
            || config.cooldown_ms > 60_000
            || !(1..=65_536).contains(&config.max_transcript_bytes)
        {
            return Err(WakeActivationError(
                "attente 1–120000 ms, cooldown ≤60000 ms, texte 1–65536 octets requis".into(),
            ));
        }
        let mut aliases = Vec::new();
        for alias in config.aliases.iter().chain(&config.vocative_prefixes) {
            if alias.is_empty()
                || alias.len() > 128
                || !alias.chars().all(is_word_character)
                || !alias.chars().any(char::is_alphabetic)
            {
                return Err(WakeActivationError(
                    "chaque alias doit être un mot de 1–128 octets contenant une lettre".into(),
                ));
            }
            let normalized = normalize_alias(alias);
            if config.aliases.contains(alias) && !aliases.contains(&normalized) {
                aliases.push(normalized);
            }
        }
        let vocative_prefixes = config
            .vocative_prefixes
            .iter()
            .map(|s| normalize_alias(s))
            .collect();
        Ok(Self {
            config,
            vocative_prefixes,
            aliases,
            state: ActivationState::Idle,
            last_time_ms: None,
            minimum_capture_ms: 0,
        })
    }

    pub fn state(&self) -> ActivationState {
        self.state
    }

    /// Avance l'horloge et expire l'attente/cooldown. Retourne vrai si l'état
    /// change. Un horodatage plus ancien est ignoré, sans réarmer quoi que ce soit.
    pub fn tick(&mut self, now_ms: u64) -> bool {
        if !self.accept_time(now_ms) {
            return false;
        }
        self.expire(now_ms)
    }

    /// Variante pour un ASR asynchrone : `captured_at_ms` désigne le début du
    /// segment, `now_ms` son traitement. Les ticks intermédiaires n'invalident
    /// pas un résultat, tandis que reset/Busy/cooldown définissent une barrière.
    pub fn on_transcript_captured(
        &mut self,
        text: &str,
        language: &str,
        captured_at_ms: u64,
        now_ms: u64,
    ) -> ActivationOutcome {
        if captured_at_ms < self.minimum_capture_ms || captured_at_ms > now_ms {
            return ignored(ActivationIgnoreReason::StaleTimestamp);
        }
        self.on_transcript_at(text, language, captured_at_ms, now_ms)
    }

    pub fn on_transcript(&mut self, text: &str, language: &str, now_ms: u64) -> ActivationOutcome {
        self.on_transcript_at(text, language, now_ms, now_ms)
    }

    fn on_transcript_at(
        &mut self,
        text: &str,
        language: &str,
        captured_at_ms: u64,
        now_ms: u64,
    ) -> ActivationOutcome {
        if !self.accept_time(now_ms) {
            return ignored(ActivationIgnoreReason::StaleTimestamp);
        }
        // La fenêtre porte sur le début de parole, pas sur la latence ASR.
        // Ne pas appeler tick pendant une transcription encore en vol.
        self.expire(captured_at_ms);
        match self.state {
            ActivationState::Busy => return ignored(ActivationIgnoreReason::Busy),
            ActivationState::Cooldown { .. } => return ignored(ActivationIgnoreReason::Cooldown),
            _ => {}
        }
        if text.len() > self.config.max_transcript_bytes || language.len() > 64 {
            return ignored(ActivationIgnoreReason::TextTooLong);
        }
        let spans = word_spans(text);
        if spans.is_empty() {
            return ignored(ActivationIgnoreReason::Empty);
        }
        let wake = spans.iter().enumerate().find(|(index, (start, end))| {
            (*index == 0
                || (*index == 1
                    && self
                        .vocative_prefixes
                        .contains(&normalize_alias(&text[spans[0].0..spans[0].1])))
                || self.config.allow_wake_anywhere)
                && self.aliases.contains(&normalize_alias(&text[*start..*end]))
        });
        let command = if let Some((_, (_, end))) = wake {
            // Seuls séparateurs et ponctuation entourant l'invocation sont
            // retirés; casse, accents, écriture et ponctuation interne restent.
            let suffix = &text[*end..];
            if word_spans(suffix).is_empty() {
                let expires_at_ms = now_ms.saturating_add(self.config.armed_timeout_ms);
                self.state = ActivationState::Armed { expires_at_ms };
                return ActivationOutcome::Armed { expires_at_ms };
            }
            suffix
                .trim_start_matches(|c: char| {
                    c.is_whitespace()
                        || matches!(
                            c,
                            '，' | ','
                                | '،'
                                | ':'
                                | '：'
                                | ';'
                                | '؛'
                                | '!'
                                | '！'
                                | '?'
                                | '？'
                                | '.'
                                | '。'
                                | '…'
                                | '»'
                                | '”'
                        )
                })
                .trim_end()
        } else if matches!(self.state, ActivationState::Armed { .. }) {
            text.trim()
        } else {
            return ignored(ActivationIgnoreReason::NoWakeWord);
        };
        self.state = ActivationState::Busy;
        self.minimum_capture_ms = now_ms;
        ActivationOutcome::Command {
            text: command.to_owned(),
            language: language.to_owned(),
        }
    }

    /// Signale une activité externe (tour clavier, lecture vocale, etc.) pour
    /// empêcher une activation concurrente. Retourne faux si le temps recule.
    pub fn set_busy(&mut self, now_ms: u64) -> bool {
        if !self.accept_time(now_ms) {
            return false;
        }
        self.state = ActivationState::Busy;
        self.minimum_capture_ms = now_ms;
        true
    }

    /// À appeler après succès OU erreur de la commande. Un appel hors Busy
    /// ne crée pas artificiellement de cooldown ni d'armement.
    pub fn finish_command(&mut self, now_ms: u64) -> bool {
        if !self.accept_time(now_ms) || self.state != ActivationState::Busy {
            return false;
        }
        self.minimum_capture_ms = now_ms.saturating_add(self.config.cooldown_ms);
        self.state = if self.config.cooldown_ms == 0 {
            ActivationState::Idle
        } else {
            ActivationState::Cooldown {
                until_ms: now_ms.saturating_add(self.config.cooldown_ms),
            }
        };
        true
    }

    /// Annulation explicite : désarme et abandonne Busy/cooldown. Le temps
    /// monotone est conservé pour rejeter les anciens résultats ASR en vol.
    pub fn reset(&mut self, now_ms: u64) -> bool {
        if !self.accept_time(now_ms) {
            return false;
        }
        self.state = ActivationState::Idle;
        self.minimum_capture_ms = now_ms;
        true
    }

    fn accept_time(&mut self, now_ms: u64) -> bool {
        if self.last_time_ms.is_some_and(|last| now_ms < last) {
            return false;
        }
        self.last_time_ms = Some(now_ms);
        true
    }

    fn expire(&mut self, now_ms: u64) -> bool {
        let expired = match self.state {
            ActivationState::Armed { expires_at_ms } => now_ms >= expires_at_ms,
            ActivationState::Cooldown { until_ms } => now_ms >= until_ms,
            _ => false,
        };
        if expired {
            self.state = ActivationState::Idle;
        }
        expired
    }
}

impl Default for WakeActivation {
    fn default() -> Self {
        Self::new(WakeActivationConfig::default()).expect("configuration statique valide")
    }
}

fn ignored(reason: ActivationIgnoreReason) -> ActivationOutcome {
    ActivationOutcome::Ignored { reason }
}

// Les marques combinantes font partie du mot : Hákille et Ha◌́kille sont
// équivalents, tandis que superHakille, Hakille2 et Hakille_autre ne le sont pas.
fn is_word_character(c: char) -> bool {
    c.is_alphanumeric()
        || matches!(c, '_' | '\u{200c}' | '\u{200d}' | '\u{2060}')
        || matches!(c as u32, 0x0300..=0x036f | 0x0591..=0x05bd | 0x05bf..=0x05c7 | 0x0610..=0x061a | 0x064b..=0x065f | 0x0670 | 0x06d6..=0x06ed | 0x0900..=0x0903 | 0x093a..=0x094f | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff | 0x20d0..=0x20ff | 0xfe20..=0xfe2f)
}

fn word_spans(text: &str) -> Vec<(usize, usize)> {
    let mut result = Vec::new();
    let mut start = None;
    for (index, character) in text.char_indices() {
        if is_word_character(character) {
            start.get_or_insert(index);
        } else if let Some(start) = start.take() {
            result.push((start, index));
        }
    }
    if let Some(start) = start {
        result.push((start, text.len()));
    }
    result
}

fn normalize_alias(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .filter(|c| !matches!(*c as u32, 0x0300..=0x036f))
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
            'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ĭ' | 'į' => 'i',
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
            _ => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_captured_while_armed_survives_slow_asr() {
        let mut activation = WakeActivation::new(WakeActivationConfig::default()).unwrap();
        assert!(matches!(
            activation.on_transcript("Hakille", "fr", 1_000),
            ActivationOutcome::Armed { .. }
        ));
        assert!(matches!(
            activation.on_transcript_captured("ouvre le calendrier", "fr", 5_000, 60_000),
            ActivationOutcome::Command { .. }
        ));
        activation.finish_command(60_000);
        assert!(matches!(
            activation.on_transcript("Hakille", "fr", 60_500),
            ActivationOutcome::Ignored { .. }
        ));
    }

    #[test]
    fn ordinary_speech_and_partial_names_do_not_activate() {
        for text in [
            "bonjour",
            "ouvre la fenêtre",
            "superHakille ouvre",
            "Hakille2",
            "Hakille_autre",
            "Hakille\u{200d}autre",
            "Hakilleé",
            "je parle de Hakille",
            "Hаkille ouvre",
        ] {
            let mut wake = WakeActivation::default();
            assert_eq!(
                wake.on_transcript(text, "fr", 0),
                ignored(ActivationIgnoreReason::NoWakeWord),
                "{text}"
            );
            assert_eq!(wake.state(), ActivationState::Idle);
        }
    }

    #[test]
    fn explicit_vocatives_allow_invocation_without_general_mentions() {
        for prefix in ["Dis", "Hey", "Salut", "Bonjour", "OK", "Hola"] {
            let mut wake = WakeActivation::default();
            assert_eq!(
                wake.on_transcript(&format!("{prefix} Hakille, ouvre /tmp/fichier"), "fr", 0),
                ActivationOutcome::Command {
                    text: "ouvre /tmp/fichier".into(),
                    language: "fr".into()
                }
            );
        }
        let mut wake = WakeActivation::default();
        assert_eq!(
            wake.on_transcript("Hakille, /tmp/fichier", "fr", 0),
            ActivationOutcome::Command {
                text: "/tmp/fichier".into(),
                language: "fr".into()
            }
        );
    }

    #[test]
    fn wake_only_arms_then_next_phrase_is_command() {
        let mut wake = WakeActivation::default();
        assert_eq!(
            wake.on_transcript("Hakille !", "fr", 100),
            ActivationOutcome::Armed {
                expires_at_ms: 10_100
            }
        );
        assert_eq!(
            wake.on_transcript("打开窗户。", "zh", 200),
            ActivationOutcome::Command {
                text: "打开窗户。".into(),
                language: "zh".into()
            }
        );
        assert_eq!(wake.state(), ActivationState::Busy);
    }

    #[test]
    fn punctuation_case_accents_and_explicit_aliases() {
        for name in ["HAKILLE", "Hákille", "Ha\u{301}kille", "Hakil", "Hakill"] {
            let mut wake = WakeActivation::default();
            assert_eq!(
                wake.on_transcript(&format!("«{name}», ouvre Café Étoile !"), "fr", 0),
                ActivationOutcome::Command {
                    text: "ouvre Café Étoile !".into(),
                    language: "fr".into()
                }
            );
        }
        let mut config = WakeActivationConfig::default();
        config.aliases.extend(["هاكيل".into(), "ハキル".into()]);
        let mut wake = WakeActivation::new(config).unwrap();
        assert_eq!(
            wake.on_transcript("هاكيل، افتح النافذة", "ar", 0),
            ActivationOutcome::Command {
                text: "افتح النافذة".into(),
                language: "ar".into()
            }
        );
        wake.reset(1);
        assert!(matches!(
            wake.on_transcript("ハキル。", "ja", 2),
            ActivationOutcome::Armed { .. }
        ));
        assert!(matches!(
            wake.on_transcript("窓を開けて", "ja", 3),
            ActivationOutcome::Command { .. }
        ));
    }

    #[test]
    fn timeout_boundary_and_rearm() {
        let mut wake = WakeActivation::default();
        wake.on_transcript("Hakille", "fr", 0);
        assert_eq!(
            wake.on_transcript("ouvre", "fr", 10_000),
            ignored(ActivationIgnoreReason::NoWakeWord)
        );
        assert!(matches!(
            wake.on_transcript("Hakille", "fr", 10_001),
            ActivationOutcome::Armed {
                expires_at_ms: 20_001
            }
        ));
        assert!(matches!(
            wake.on_transcript("Hakille", "fr", 11_000),
            ActivationOutcome::Armed {
                expires_at_ms: 21_000
            }
        ));
        assert!(!wake.tick(20_999));
        assert!(wake.tick(21_000));
        assert_eq!(wake.state(), ActivationState::Idle);
    }

    #[test]
    fn busy_cooldown_and_explicit_reset() {
        let mut wake = WakeActivation::default();
        wake.on_transcript("Hakille ouvre", "fr", 0);
        assert_eq!(
            wake.on_transcript("Hakille encore", "fr", 1),
            ignored(ActivationIgnoreReason::Busy)
        );
        assert!(wake.finish_command(100));
        assert_eq!(
            wake.on_transcript("Hakille encore", "fr", 1099),
            ignored(ActivationIgnoreReason::Cooldown)
        );
        assert!(matches!(
            wake.on_transcript("Hakille encore", "fr", 1100),
            ActivationOutcome::Command { .. }
        ));
        assert!(wake.reset(1101));
        assert_eq!(wake.state(), ActivationState::Idle);
        assert!(!wake.finish_command(1102));
        assert!(wake.set_busy(1103));
        assert_eq!(wake.state(), ActivationState::Busy);
    }

    #[test]
    fn late_transcripts_cannot_rearm_after_reset() {
        let mut wake = WakeActivation::default();
        wake.reset(100);
        assert_eq!(
            wake.on_transcript("Hakille ouvre", "fr", 99),
            ignored(ActivationIgnoreReason::StaleTimestamp)
        );
        assert!(!wake.tick(1));
        assert_eq!(wake.state(), ActivationState::Idle);
    }

    #[test]
    fn async_results_use_capture_barrier_without_conflicting_with_ticks() {
        let mut wake = WakeActivation::default();
        wake.reset(100);
        wake.tick(300);
        assert!(matches!(
            wake.on_transcript_captured("Hakille ouvre", "fr", 200, 400),
            ActivationOutcome::Command { .. }
        ));
        wake.finish_command(500);
        wake.tick(1500);
        assert_eq!(
            wake.on_transcript_captured("Hakille ouvre", "fr", 450, 1600),
            ignored(ActivationIgnoreReason::StaleTimestamp)
        );
        assert!(matches!(
            wake.on_transcript_captured("Hakille ouvre", "fr", 1500, 1600),
            ActivationOutcome::Command { .. }
        ));
        wake.reset(1700);
        assert_eq!(
            wake.on_transcript_captured("Hakille ouvre", "fr", 1699, 1800),
            ignored(ActivationIgnoreReason::StaleTimestamp)
        );
    }

    #[test]
    fn invalid_configuration_and_oversized_input_are_rejected() {
        for alias in ["", "ha kille", "!!!", "123"] {
            let config = WakeActivationConfig {
                aliases: vec![alias.into()],
                ..Default::default()
            };
            assert!(WakeActivation::new(config).is_err());
        }
        let mut wake = WakeActivation::default();
        assert_eq!(
            wake.on_transcript(&"a".repeat(8193), "fr", 0),
            ignored(ActivationIgnoreReason::TextTooLong)
        );
        assert_eq!(
            wake.on_transcript(" ! … ", "fr", 1),
            ignored(ActivationIgnoreReason::Empty)
        );
        assert_eq!(wake.state(), ActivationState::Idle);
    }

    #[test]
    fn wake_anywhere_requires_explicit_configuration() {
        let mut wake = WakeActivation::new(WakeActivationConfig {
            allow_wake_anywhere: true,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(
            wake.on_transcript("bonjour Hakille, ouvre", "fr", 0),
            ActivationOutcome::Command {
                text: "ouvre".into(),
                language: "fr".into()
            }
        );
    }
}
