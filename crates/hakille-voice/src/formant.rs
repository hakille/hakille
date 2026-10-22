//! # Synthétiseur formantique RÉEL (Klatt simplifié, pur Rust, std uniquement)
//!
//! VRAIE synthèse vocale (aucune sinusoïde, aucun checkpoint neural requis) :
//!
//! 1. **Phonémisation FR/EN par règles** (~120 règles grapheme→phonème,
//!    étiquetées `R001…` ci-dessous, pas de modèle) : digraphes (`ch`, `gn`,
//!    `eau`, `oin`…), nasales contextuelles, `e` muet/caduc, liaisons
//!    simplifiées, exceptions lexicales documentées (`oignon`, `ville`…),
//!    mode anglais par mot (détection `th/sh/oo/ee/w/k/ing…`).
//! 2. **Source glottique** : train d'impulsions à F0 paramétrable (80–300 Hz,
//!    pulse de Rosenberg différencié) + souffle d'aspiration ; bruit blanc
//!    pour les non-voisées.
//! 3. **5 formants interpolés** par phonème (table F1–F5 voyelles/consonnes,
//!    cascade de résonateurs du 2nd ordre, interpolation ~6 ms aux frontières).
//! 4. **Enveloppe prosodique** : déclinaison, accent final FR, pauses
//!    (`,.!?;:`), montée interrogative, emphase (`!`, capitales).
//!
//! ## REAL vs v1 (documenté)
//! - REAL : phonémisation effective, source pulsée à F0 mesurable,
//!   résonateurs formantiques, prosodie — sortie 24 kHz intelligible
//!   (timbre robotique assumé).
//! - v1 (documenté) : pas de coarticulation fine au-delà de l'interpolation,
//!   pas de diphtongues EN (approximées), débit/lexique perfectibles.
//!   La bascule neurale (CosyVoice2/NeuTTS) reste documentée dans `tts.rs`
//!   via `provision_neural_checkpoint()`.

pub const FORMANT_SAMPLE_RATE: u32 = 24_000;
pub const FORMANT_MIN_F0: f32 = 80.0;
pub const FORMANT_MAX_F0: f32 = 300.0;

/// Largeurs de bande globales F1–F5 (Hz), multipliées par `bw_scale`.
const FORMANT_BW: [f32; 5] = [70.0, 90.0, 130.0, 180.0, 250.0];

// ---------------------------------------------------------------------------
// Voix
// ---------------------------------------------------------------------------

/// Paramètres de voix (F0 en Hz, clampée 80–300).
#[derive(Debug, Clone)]
pub struct FormantVoice {
    pub f0_base_hz: f32,
    pub rate: f32,
    pub breathiness: f32,
    pub gain: f32,
}

impl Default for FormantVoice {
    fn default() -> Self {
        Self {
            f0_base_hz: 120.0,
            rate: 1.0,
            breathiness: 0.15,
            gain: 0.8,
        }
    }
}

impl FormantVoice {
    pub fn new(f0_base_hz: f32, rate: f32) -> Self {
        Self {
            f0_base_hz: f0_base_hz.clamp(FORMANT_MIN_F0, FORMANT_MAX_F0),
            rate: rate.clamp(0.5, 2.0),
            breathiness: 0.15,
            gain: 0.8,
        }
    }
}

// ---------------------------------------------------------------------------
// Phonèmes
// ---------------------------------------------------------------------------

/// Phonème : cible formantique + prosodie locale.
/// `sym` utilise des clés ASCII (`E`=ɛ, `O`=ɔ, `S`=ʃ, `Z`=ʒ, `R`=ʁ…).
#[derive(Debug, Clone)]
pub struct Phoneme {
    pub sym: &'static str,
    pub formants: [f32; 5],
    pub bw_scale: f32,
    pub dur_ms: f32,
    pub voiced: bool,
    pub noise_gain: f32,
}

fn ph(
    sym: &'static str,
    f: [f32; 5],
    dur_ms: f32,
    voiced: bool,
    noise_gain: f32,
    bw_scale: f32,
) -> Phoneme {
    Phoneme {
        sym,
        formants: f,
        bw_scale,
        dur_ms,
        voiced,
        noise_gain,
    }
}

/// Table formantique F1–F5 (valeurs Klatt simplifiées, documentées v1).
pub fn phoneme_params(sym: &str) -> Phoneme {
    match sym {
        // Voyelles orales FR.
        "a" => ph(
            "a",
            [730.0, 1090.0, 2440.0, 3500.0, 4500.0],
            90.0,
            true,
            0.0,
            1.0,
        ),
        "i" => ph(
            "i",
            [270.0, 2290.0, 3010.0, 3500.0, 4500.0],
            90.0,
            true,
            0.0,
            1.0,
        ),
        "e" => ph(
            "e",
            [450.0, 1930.0, 2560.0, 3500.0, 4500.0],
            90.0,
            true,
            0.0,
            1.0,
        ),
        "E" => ph(
            "E",
            [610.0, 1720.0, 2410.0, 3500.0, 4500.0],
            95.0,
            true,
            0.0,
            1.0,
        ),
        "o" => ph(
            "o",
            [500.0, 890.0, 2410.0, 3500.0, 4500.0],
            90.0,
            true,
            0.0,
            1.0,
        ),
        "O" => ph(
            "O",
            [560.0, 900.0, 2560.0, 3400.0, 4400.0],
            95.0,
            true,
            0.0,
            1.0,
        ),
        "u" => ph(
            "u",
            [300.0, 870.0, 2240.0, 3500.0, 4500.0],
            90.0,
            true,
            0.0,
            1.0,
        ),
        "y" => ph(
            "y",
            [300.0, 1750.0, 2150.0, 3500.0, 4500.0],
            90.0,
            true,
            0.0,
            1.0,
        ),
        "@" => ph(
            "@",
            [500.0, 1500.0, 2500.0, 3500.0, 4500.0],
            55.0,
            true,
            0.0,
            1.0,
        ),
        "oe" => ph(
            "oe",
            [560.0, 1400.0, 2530.0, 3500.0, 4500.0],
            90.0,
            true,
            0.0,
            1.0,
        ),
        "eu" => ph(
            "eu",
            [450.0, 1500.0, 2500.0, 3500.0, 4500.0],
            90.0,
            true,
            0.0,
            1.0,
        ),
        // Voyelles nasales FR.
        "an" => ph(
            "an",
            [700.0, 1150.0, 2500.0, 3500.0, 4500.0],
            110.0,
            true,
            0.0,
            1.0,
        ),
        "in" => ph(
            "in",
            [550.0, 1650.0, 2450.0, 3500.0, 4500.0],
            110.0,
            true,
            0.0,
            1.0,
        ),
        "on" => ph(
            "on",
            [480.0, 980.0, 2600.0, 3500.0, 4500.0],
            110.0,
            true,
            0.0,
            1.0,
        ),
        "un" => ph(
            "un",
            [550.0, 1400.0, 2500.0, 3500.0, 4500.0],
            110.0,
            true,
            0.0,
            1.0,
        ),
        // Voyelles EN.
        "I" => ph(
            "I",
            [400.0, 1900.0, 2550.0, 3500.0, 4500.0],
            85.0,
            true,
            0.0,
            1.0,
        ),
        "ae" => ph(
            "ae",
            [700.0, 1700.0, 2500.0, 3500.0, 4500.0],
            95.0,
            true,
            0.0,
            1.0,
        ),
        "A" => ph(
            "A",
            [640.0, 1200.0, 2500.0, 3500.0, 4500.0],
            90.0,
            true,
            0.0,
            1.0,
        ),
        "U" => ph(
            "U",
            [450.0, 1100.0, 2350.0, 3400.0, 4400.0],
            85.0,
            true,
            0.0,
            1.0,
        ),
        // Occlusives sourdes / voisées.
        "p" => ph(
            "p",
            [500.0, 1200.0, 2500.0, 3500.0, 4500.0],
            75.0,
            false,
            1.0,
            1.3,
        ),
        "t" => ph(
            "t",
            [400.0, 1800.0, 2900.0, 3500.0, 4500.0],
            75.0,
            false,
            1.0,
            1.3,
        ),
        "k" => ph(
            "k",
            [400.0, 1500.0, 2400.0, 3500.0, 4500.0],
            75.0,
            false,
            1.0,
            1.3,
        ),
        "b" => ph(
            "b",
            [500.0, 1200.0, 2500.0, 3500.0, 4500.0],
            60.0,
            true,
            0.5,
            1.3,
        ),
        "d" => ph(
            "d",
            [400.0, 1800.0, 2900.0, 3500.0, 4500.0],
            60.0,
            true,
            0.5,
            1.3,
        ),
        "g" => ph(
            "g",
            [400.0, 1500.0, 2400.0, 3500.0, 4500.0],
            60.0,
            true,
            0.5,
            1.3,
        ),
        // Fricatives.
        "f" => ph(
            "f",
            [400.0, 1300.0, 2500.0, 3500.0, 4500.0],
            120.0,
            false,
            0.9,
            1.6,
        ),
        "s" => ph(
            "s",
            [350.0, 2000.0, 3200.0, 4500.0, 5500.0],
            130.0,
            false,
            1.0,
            1.6,
        ),
        "S" => ph(
            "S",
            [350.0, 1700.0, 2500.0, 3500.0, 4500.0],
            130.0,
            false,
            1.0,
            1.6,
        ),
        "v" => ph(
            "v",
            [400.0, 1300.0, 2500.0, 3500.0, 4500.0],
            110.0,
            true,
            0.6,
            1.6,
        ),
        "z" => ph(
            "z",
            [350.0, 2000.0, 3200.0, 4500.0, 5500.0],
            120.0,
            true,
            0.6,
            1.6,
        ),
        "Z" => ph(
            "Z",
            [350.0, 1700.0, 2500.0, 3500.0, 4500.0],
            120.0,
            true,
            0.6,
            1.6,
        ),
        "tS" => ph(
            "tS",
            [400.0, 1800.0, 2700.0, 3500.0, 4500.0],
            130.0,
            false,
            1.0,
            1.6,
        ),
        "th" => ph(
            "th",
            [400.0, 1500.0, 2600.0, 3500.0, 4500.0],
            120.0,
            false,
            0.9,
            1.6,
        ),
        "dh" => ph(
            "dh",
            [400.0, 1500.0, 2600.0, 3500.0, 4500.0],
            110.0,
            true,
            0.5,
            1.6,
        ),
        "h" => ph(
            "h",
            [500.0, 1500.0, 2500.0, 3500.0, 4500.0],
            100.0,
            false,
            0.7,
            1.4,
        ),
        // Nasales / liquides / glides.
        "m" => ph(
            "m",
            [280.0, 900.0, 2200.0, 3300.0, 4300.0],
            80.0,
            true,
            0.0,
            1.0,
        ),
        "n" => ph(
            "n",
            [280.0, 1700.0, 2500.0, 3300.0, 4300.0],
            80.0,
            true,
            0.0,
            1.0,
        ),
        "gn" => ph(
            "gn",
            [300.0, 1900.0, 2600.0, 3400.0, 4400.0],
            80.0,
            true,
            0.05,
            1.0,
        ),
        "ng" => ph(
            "ng",
            [300.0, 2100.0, 2800.0, 3400.0, 4400.0],
            80.0,
            true,
            0.0,
            1.0,
        ),
        "l" => ph(
            "l",
            [400.0, 1600.0, 2400.0, 3300.0, 4300.0],
            70.0,
            true,
            0.03,
            1.0,
        ),
        "R" => ph(
            "R",
            [350.0, 1300.0, 1700.0, 2600.0, 3500.0],
            75.0,
            true,
            0.22,
            1.2,
        ),
        "j" => ph(
            "j",
            [260.0, 2100.0, 2900.0, 3500.0, 4500.0],
            65.0,
            true,
            0.03,
            1.0,
        ),
        "w" => ph(
            "w",
            [300.0, 900.0, 2200.0, 3300.0, 4300.0],
            65.0,
            true,
            0.03,
            1.0,
        ),
        "hu" => ph(
            "hu",
            [300.0, 1800.0, 2200.0, 3400.0, 4400.0],
            65.0,
            true,
            0.03,
            1.0,
        ),
        _ => ph(
            "pau",
            [500.0, 1500.0, 2500.0, 3500.0, 4500.0],
            80.0,
            false,
            0.0,
            1.0,
        ),
    }
}

fn is_voiced_sym(sym: &str) -> bool {
    phoneme_params(sym).voiced
}

// ---------------------------------------------------------------------------
// Normalisation + phonémisation
// ---------------------------------------------------------------------------

fn is_vowel_letter(c: char) -> bool {
    matches!(
        c,
        'a' | 'à'
            | 'â'
            | 'e'
            | 'è'
            | 'é'
            | 'ê'
            | 'ë'
            | 'i'
            | 'î'
            | 'ï'
            | 'o'
            | 'ô'
            | 'u'
            | 'ù'
            | 'û'
            | 'y'
            | 'œ'
            | 'æ'
    )
}

fn is_consonant_letter(c: char) -> bool {
    c.is_alphabetic() && !is_vowel_letter(c) && c != 'h'
}

fn expand_digits(word: &str) -> String {
    const DIGITS: [&str; 10] = [
        "zéro", "un", "deux", "trois", "quatre", "cinq", "six", "sept", "huit", "neuf",
    ];
    let mut out = String::new();
    for c in word.chars() {
        if let Some(d) = c.to_digit(10) {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(DIGITS[d as usize]);
        } else {
            out.push(c);
        }
    }
    out
}

/// Heuristique de langue par mot : motifs typiquement anglais ou lexique EN,
/// avec garde-fous FR (homographes : est/et/ou/un/en/in/an/as/or…).
fn word_is_english(word: &str) -> bool {
    const FR_GUARDS: &[&str] = &[
        "a", "y", "et", "est", "ou", "un", "une", "en", "on", "in", "an", "as", "or", "os", "il",
        "ils", "elle", "nous", "vous", "je", "tu", "le", "la", "les", "de", "des", "du", "au",
        "aux", "ce", "ces", "mon", "ton", "son", "ma", "ta", "sa", "mes", "tes", "ses", "ne", "se",
        "que", "qui", "quoi", "dont", "tout", "tous", "plus", "moins", "avec", "sans", "sous",
        "sur", "dans", "par", "pour", "vers", "chez", "mais", "donc", "car", "ni", "si", "oui",
        "non",
    ];
    if FR_GUARDS.contains(&word) {
        return false;
    }
    const EN_LEXICON: &[&str] = &[
        "the", "this", "that", "these", "those", "them", "then", "than", "though", "of", "and",
        "hello", "hi", "house", "how", "who", "what", "when", "where", "world", "thanks", "thank",
        "you", "your", "yes", "please", "sorry", "good", "morning", "evening", "night", "day",
        "man", "woman", "boy", "girl", "time", "work", "over", "under", "with", "from", "have",
        "has", "are", "was", "were", "will", "would", "should", "now", "here", "there", "very",
        "more", "most", "some", "any", "we", "they", "his", "her", "our", "out", "up", "down",
        "do", "does", "did", "can", "not", "no", "for", "on", "at", "to", "be", "is",
    ];
    if EN_LEXICON.contains(&word) {
        return true;
    }
    word.contains('w')
        || word.contains('k')
        || word.contains("th")
        || word.contains("sh")
        || word.contains("oo")
        || word.contains("ee")
        || word.contains("ea")
        || word.contains("oa")
        || word.contains("ck")
        || word.contains("ing")
        || word.contains("ay")
        || word.contains("oy")
}

/// Marqueur interne d'accentuation d'une majuscule (R005) : consommé par
/// `build_timeline`, filtré de l'API publique [`phonemize`].
const CAP_MARK: &str = "cap*";

/// Phonémise un texte FR/EN en symboles (API publique, sans marqueurs
/// internes d'accentuation). Règles numérotées R001… (v1).
pub fn phonemize(text: &str) -> Vec<&'static str> {
    let mut out = phonemize_with_prosody(text);
    out.retain(|s| *s != CAP_MARK);
    out
}

/// Phonémisation interne : identique à [`phonemize`] mais conserve le
/// marqueur `cap*` devant chaque mot accentué (majuscule).
fn phonemize_with_prosody(text: &str) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    // Normalisation : minuscules, chiffres → mots, apostrophes élidées.
    let mut norm = String::new();
    for c in text.chars() {
        if c == '\'' || c == '’' {
            continue; // R001 : élision (l'homme → lhomme ≈ /lOm/).
        } else if c.is_alphabetic() || c.is_ascii_digit() {
            // R001b : la casse est conservée pour détecter l'accentuation des
            // majuscules ; la mise en minuscules se fait par mot avant le G2P.
            norm.push(c);
        } else if c == ',' || c == '.' || c == '!' || c == '?' || c == ';' || c == ':' {
            norm.push(c);
        } else {
            norm.push(' ');
        }
    }
    let expanded = expand_digits(&norm); // R002 : chiffres → mots.
    // Barrières de phrase pour la prosodie : on garde la ponctuation forte.
    let mut boundary: Option<char> = None;
    for token in expanded.split_whitespace() {
        // Sépare ponctuation forte éventuelle en fin de token.
        let (word, punct) = match token.strip_suffix(|c| ",.!?;:".contains(c)) {
            Some(_) => {
                let p = token.chars().last().unwrap();
                (&token[..token.len() - p.len_utf8()], Some(p))
            }
            None => (token, None),
        };
        if word.is_empty() {
            if let Some(p) = punct {
                push_pause(&mut out, p);
                boundary = Some(p);
            }
            continue;
        }
        // R005 : mot accentué (majuscule présente) → marqueur avant le mot.
        let word_lc = word.to_lowercase();
        if word.chars().any(|c| c.is_uppercase()) {
            out.push(CAP_MARK);
        }
        let w: Vec<char> = word_lc.chars().collect();
        if word_is_english(&word_lc) {
            g2p_en_word(&w, &mut out); // R003 : branche anglaise par mot.
        } else {
            g2p_fr_word(&w, &mut out);
        }
        // Micro-pause inter-mots (30 ms) pour l'intelligibilité.
        out.push("pau30");
        if let Some(p) = punct {
            push_pause(&mut out, p); // R004 : pauses ,.!?;:
            boundary = Some(p);
        }
        let _ = boundary;
    }
    // Marqueur final : le dernier "pau30" devient pause de fin.
    out
}

fn push_pause(out: &mut Vec<&'static str>, p: char) {
    match p {
        ',' | ';' | ':' => out.push("pau90"),
        '.' | '!' | '?' => {
            out.push("pau180");
            out.push(match p {
                '?' => "rise?",
                '!' => "boost!",
                _ => "fall.",
            });
        }
        _ => {}
    }
}

/// G2P français. Chaque branche est une règle (R010…).
fn g2p_fr_word(w: &[char], out: &mut Vec<&'static str>) {
    // R010 : exception oignon (oi ne s'applique pas).
    if w.iter().collect::<String>() == "oignon" || w.iter().collect::<String>() == "oignons" {
        out.extend(["O", "gn", "on"]);
        return;
    }
    // R011 : monsieur → /m@sjø/.
    if w.iter().collect::<String>() == "monsieur" {
        out.extend(["m", "@", "s", "j", "eu"]);
        return;
    }
    // R012 : femme → /fam/.
    if w.iter().collect::<String>() == "femme" || w.iter().collect::<String>() == "femmes" {
        out.extend(["f", "a", "m"]);
        return;
    }
    // R013 : second → /s@gOn/.
    if w.iter().collect::<String>().starts_with("second") {
        out.extend(["s", "@", "g", "O", "n"]);
        let _ = w;
        return;
    }
    let n = w.len();
    let mut i = 0;
    // Dit si le mot finit par "er" prononcé (R014 : exceptions -er).
    let word: String = w.iter().collect();
    let er_sounds: bool = [
        "mer", "fer", "hier", "super", "hyper", "enfer", "hiver", "amer", "cancer", "hamster",
        "leader", "roller", "cutter", "poster",
    ]
    .contains(&word.as_str());
    // R015 : -ille → /il/ après consonne dure (ville, mille…).
    let ill_oral: bool = word == "ville"
        || word.ends_with("ville")
        || word == "mille"
        || word == "milles"
        || word == "tranquille"
        || word == "tranquilles"
        || word == "lille"
        || word == "pupille"
        || word == "pupilles"
        || word.starts_with("distill")
        || word.starts_with("oscill");
    // R016 : ch grec → /k/ (orchestre, archange…).
    let ch_is_k: bool = word.starts_with("orch")
        || word.starts_with("archange")
        || word.starts_with("archaï")
        || word.starts_with("archai")
        || word.starts_with("orchid");
    // R017 : gn → /gn/ (gnome, diagnostic…).
    let gn_is_two: bool = word.starts_with("gno")
        || word.contains("diag")
        || word.contains("stag")
        || word.contains("digm");
    // R018 : finales muettes lexicales.
    let l_silent: bool = [
        "outil", "outils", "gentil", "gentils", "fusil", "fusils", "sourcil", "persil", "baril",
        "chenil",
    ]
    .contains(&word.as_str());
    let s_sounds: bool = ["fils", "sens", "ours", "os", "tous"].contains(&word.as_str());
    let b_sounds: bool = word == "club" || word == "snob" || word == "clubs";
    let at = |j: usize| -> char { if j < n { w[j] } else { '\0' } };
    let starts = |j: usize, s: &str| -> bool {
        let v: Vec<char> = s.chars().collect();
        j + v.len() <= n && w[j..j + v.len()] == v[..]
    };
    let vowel_at = |j: usize| -> bool { j < n && is_vowel_letter(w[j]) };
    while i < n {
        let c = w[i];
        let end1 = i + 1 >= n; // dernier caractère.
        // --- Groupes de 4-5 lettres ---
        if starts(i, "eaux") {
            out.push("o");
            i += 4;
            continue;
        } // R020
        // --- Groupes de 3 lettres ---
        if starts(i, "eau") {
            out.push("o");
            i += 3;
            continue;
        } // R021
        if starts(i, "aux") && i + 3 >= n {
            out.push("o");
            i += 3;
            continue;
        } // R022 : -aux final.
        if starts(i, "oin") {
            out.push("w");
            out.push("in");
            i += 3;
            continue;
        } // R023
        if starts(i, "ien") {
            out.push("j");
            out.push("in");
            i += 3;
            continue;
        } // R024 : chien.
        if starts(i, "ion") && i + 3 >= n {
            out.push("j");
            out.push("on");
            i += 3;
            continue;
        } // R025 : nation.
        if starts(i, "aim") && vowel_at(i + 3) {
            out.push("e");
            out.push("m");
            i += 3;
            continue;
        } // R026 : aimer.
        if starts(i, "aim") {
            out.push("in");
            i += 3;
            continue;
        } // R027 : faim.
        if starts(i, "ain") && vowel_at(i + 3) {
            out.push("E");
            out.push("n");
            i += 3;
            continue;
        } // R028 : laine.
        if starts(i, "ein") && vowel_at(i + 3) {
            out.push("E");
            out.push("n");
            i += 3;
            continue;
        } // R029 : peine.
        if starts(i, "ain") || starts(i, "ein") {
            out.push("in");
            i += 3;
            continue;
        } // R030
        if starts(i, "aill") {
            out.push("a");
            out.push("j");
            i += 4;
            continue;
        } // R031 : taille.
        if starts(i, "eill") {
            out.push("E");
            out.push("j");
            i += 4;
            continue;
        } // R032 : soleil.
        if starts(i, "ouill") {
            out.push("u");
            out.push("j");
            i += 5;
            continue;
        } // R033 : mouiller.
        if starts(i, "ueill") {
            out.push("oe");
            out.push("j");
            i += 5;
            continue;
        } // R034 : cueille.
        if starts(i, "ille") && ill_oral {
            out.push("i");
            out.push("l");
            i += 4;
            continue;
        } // R035 : ville.
        if starts(i, "ille") || starts(i, "ill") && i > 0 && is_vowel_letter(w[i - 1]) {
            out.push("j"); // R036 : fille, bastille (ill après voyelle → /j/).
            i += if starts(i, "ille") { 4 } else { 3 };
            continue;
        }
        if starts(i, "ill") {
            out.push("i");
            out.push("l");
            i += 3;
            continue;
        } // R037 : défaut /il/.
        if starts(i, "chr") {
            out.push("k");
            out.push("R");
            i += 3;
            continue;
        } // R038 : chrétien.
        if starts(i, "chl") {
            out.push("k");
            out.push("l");
            i += 3;
            continue;
        } // R039 : chlore.
        if starts(i, "sch") {
            out.push("S");
            i += 3;
            continue;
        } // R040 : schéma.
        if starts(i, "rch") && ch_is_k {
            out.push("R");
            out.push("k");
            i += 3;
            continue;
        } // R041 : orchestre.
        if starts(i, "quo") && i + 3 < n && vowel_at(i + 3) && starts(i, "quoi") {
            out.push("k");
            out.push("w");
            out.push("a");
            i += 4;
            continue; // R042 : quoi.
        }
        if starts(i, "qua") && vowel_at(i + 3) {
            out.push("k");
            out.push("w");
            i += 2;
            continue;
        } // R043 : équateur.
        if starts(i, "gue") || starts(i, "gui") {
            // R044 : fatigue, guide → /g/ ; aiguille → /gɥ/.
            if i + 3 >= n || !vowel_at(i + 3) {
                out.push("g");
                i += 3;
                continue;
            }
            out.push("g");
            out.push("hu");
            i += 2;
            continue;
        }
        if starts(i, "gua") || starts(i, "guo") {
            out.push("g");
            out.push("w");
            i += 2;
            continue;
        } // R045
        if starts(i, "gea") || starts(i, "geo") {
            out.push("Z");
            i += 2;
            continue;
        } // R046 : mangea.
        if starts(i, "est") && i + 3 >= n && word != "est" {
            out.push("E");
            out.push("s");
            out.push("t");
            i += 3;
            continue;
        } // R047 : ouest.
        if starts(i, "ent") && i + 3 >= n {
            out.push("an");
            i += 3;
            continue;
        } // R048 : vent (verbes 3pl approximés).
        // --- Digraphes consonnes ---
        if starts(i, "ch") {
            out.push("S");
            i += 2;
            continue;
        } // R050 : chat.
        if starts(i, "ph") {
            out.push("f");
            i += 2;
            continue;
        } // R051
        if starts(i, "sh") {
            out.push("S");
            i += 2;
            continue;
        } // R052 : short (emprunt).
        if starts(i, "th") {
            out.push("t");
            i += 2;
            continue;
        } // R053 : théâtre.
        if starts(i, "gn") {
            if gn_is_two {
                out.push("g");
                out.push("n");
            }
            // R054
            else {
                out.push("gn");
            } // R055 : montagne.
            i += 2;
            continue;
        }
        if starts(i, "qu") {
            out.push("k");
            i += 2;
            continue;
        } // R056 (qua/quoi traités avant).
        if starts(i, "gu") && (at(i + 2) == 'a' || at(i + 2) == 'o') {
            out.push("g");
            out.push("w");
            i += 2;
            continue;
        } // R057
        if starts(i, "gu") && vowel_at(i + 2) {
            out.push("g");
            out.push("hu");
            i += 2;
            continue;
        } // R058
        if starts(i, "ge") || starts(i, "gi") || starts(i, "gy") {
            out.push("Z");
            i += 2;
            continue;
        } // R059
        if starts(i, "gg") && matches!(at(i + 2), 'e' | 'é' | 'è' | 'i') {
            out.push("g");
            out.push("Z");
            i += 2;
            continue;
        } // R060 : suggère.
        if starts(i, "gg") {
            out.push("g");
            i += 2;
            continue;
        } // R061
        if starts(i, "cc") && matches!(at(i + 2), 'e' | 'é' | 'è' | 'i' | 'î' | 'y') {
            out.push("k");
            out.push("s");
            i += 2;
            continue;
        } // R062 : accent.
        if starts(i, "cc") {
            out.push("k");
            i += 2;
            continue;
        } // R063 : accord.
        if starts(i, "ck") {
            out.push("k");
            i += 2;
            continue;
        } // R064
        if starts(i, "sc") && matches!(at(i + 2), 'e' | 'é' | 'è' | 'i' | 'î' | 'y') {
            out.push("s");
            i += 2;
            continue;
        } // R065 : science.
        if starts(i, "ss") {
            out.push("s");
            i += 2;
            continue;
        } // R066 : poisson.
        if starts(i, "tt")
            || starts(i, "dd")
            || starts(i, "ff")
            || starts(i, "ll")
            || starts(i, "mm")
            || starts(i, "nn")
            || starts(i, "pp")
            || starts(i, "rr")
        {
            // R067 : géminées simplifiées (une seule).
            out.push(match c {
                't' => "t",
                'd' => "d",
                'f' => "f",
                'l' => "l",
                'm' => "m",
                'n' => "n",
                'p' => "p",
                _ => "R",
            });
            i += 2;
            continue;
        }
        if starts(i, "zz") {
            out.push("z");
            i += 2;
            continue;
        } // R068
        if starts(i, "ps") && i == 0 {
            out.push("s");
            i += 2;
            continue;
        } // R069 : psychologie.
        if starts(i, "pt") {
            out.push("t");
            i += 2;
            continue;
        } // R070 : temps, sept.
        if starts(i, "ct") && i + 2 >= n {
            out.push("k");
            out.push("t");
            i += 2;
            continue;
        } // R071 : contact.
        if starts(i, "mn") {
            out.push("n");
            i += 2;
            continue;
        } // R072 : automne, damner.
        if starts(i, "mne") && i + 3 >= n {
            out.push("n");
            i += 3;
            continue;
        } // R073
        if starts(i, "ti") && at(i + 2) == 'o' {
            out.push("s");
            out.push("j");
            i += 2;
            continue;
        } // R074 : nation.
        if starts(i, "ti") && vowel_at(i + 2) && at(i + 2) != 'e' {
            out.push("t");
            out.push("j");
            i += 2;
            continue;
        } // R075 : pitié /pitje/.
        if starts(i, "ex") && vowel_at(i + 2) && i == 0 {
            out.push("E");
            out.push("g");
            out.push("z");
            i += 2;
            continue;
        } // R076 : examen.
        // --- Voyelles et nasales ---
        if starts(i, "au") {
            out.push("o");
            i += 2;
            continue;
        } // R080 : chaud.
        if starts(i, "ai")
            && (starts(i, "ais") || starts(i, "ait") || starts(i, "aient"))
            && i + 3 >= n.saturating_sub(3)
        {
            out.push("E"); // R081 : -ais/-ait/-aient → /E/.
            i += if starts(i, "aient") { 5 } else { 3 };
            continue;
        }
        if starts(i, "aî") {
            out.push("E");
            i += 2;
            continue;
        } // R082 : maître.
        if starts(i, "ai") {
            out.push("E");
            i += 2;
            continue;
        } // R083 : lait (≈ /lE/, robotique assumé).
        if starts(i, "ei") {
            out.push("E");
            i += 2;
            continue;
        } // R084 : peine → /pEn/ via R029 sinon.
        if starts(i, "oi") {
            out.push("w");
            out.push("a");
            i += 2;
            continue;
        } // R085 : moi, trois.
        if starts(i, "oî") {
            out.push("w");
            out.push("a");
            i += 2;
            continue;
        } // R086
        if starts(i, "ou") && vowel_at(i + 2) {
            out.push("w");
            i += 2;
            continue;
        } // R087 : oui, jouer.
        if starts(i, "ou") {
            out.push("u");
            i += 2;
            continue;
        } // R088 : loup.
        if starts(i, "où") {
            out.push("u");
            i += 2;
            continue;
        } // R089
        if starts(i, "eu") {
            // R090 : bleu→/ø/, beurre→/œ/.
            if i + 2 >= n || at(i + 2) == 'e' || at(i + 2) == 'u' {
                out.push("eu");
            } else if at(i + 2) == 'r' || is_consonant_letter(at(i + 2)) {
                out.push("oe");
            } else {
                out.push("eu");
            }
            i += 2;
            continue;
        }
        if starts(i, "eux") && i + 3 >= n {
            out.push("eu");
            i += 3;
            continue;
        } // R091 : deux.
        if starts(i, "œu") {
            out.push("oe");
            i += 2;
            continue;
        } // R092 : cœur.
        if starts(i, "an") || starts(i, "en") {
            // R093 : nasal sauf +voyelle (banane), double (panne), ou "en"+n.
            if vowel_at(i + 2) {
                out.push("a");
                i += 1;
                continue;
            }
            if at(i + 2) == 'n' || at(i + 2) == 'm' {
                out.push("a");
                i += 1;
                continue;
            }
            out.push("an");
            i += 2;
            continue;
        }
        if starts(i, "am") || starts(i, "em") {
            if vowel_at(i + 2) {
                out.push("a");
                i += 1;
                continue;
            } // R094 : rame.
            if at(i + 2) == 'm' || at(i + 2) == 'n' {
                // R095 : femme→/am/, gramme ; "emme"→/am/.
                out.push("a");
                i += 1;
                continue;
            }
            out.push("an");
            i += 2;
            continue; // R096 : champ.
        }
        if starts(i, "on") || starts(i, "om") {
            if vowel_at(i + 2) {
                out.push("O");
                i += 1;
                continue;
            } // R097 : bonheur.
            if at(i + 2) == 'n' || at(i + 2) == 'm' {
                out.push("O");
                i += 1;
                continue;
            } // R098 : bonne.
            out.push("on");
            i += 2;
            continue; // R099 : bon.
        }
        if starts(i, "in") || starts(i, "yn") {
            if vowel_at(i + 2) {
                out.push("i");
                i += 1;
                continue;
            } // R100 : inutile.
            if at(i + 2) == 'n' || at(i + 2) == 'm' {
                out.push("i");
                i += 1;
                continue;
            } // R101 : inné.
            out.push("in");
            i += 2;
            continue; // R102 : vin.
        }
        if starts(i, "im") {
            if vowel_at(i + 2) {
                out.push("i");
                i += 1;
                continue;
            } // R103 : image.
            if at(i + 2) == 'm' {
                out.push("i");
                i += 1;
                continue;
            }
            out.push("in");
            i += 2;
            continue; // R104 : simple.
        }
        if starts(i, "un") {
            if i + 2 >= n {
                out.push("in");
                i += 2;
                continue;
            } // R105 : tribun → /tribin/ (fusion assumée).
            if starts(i, "une") {
                out.push("y");
                out.push("n");
                i += 3;
                continue;
            } // R106 : une.
            if vowel_at(i + 2) {
                out.push("y");
                i += 1;
                continue;
            } // R107 : univers.
            out.push("un");
            i += 2;
            continue; // R108 : brun.
        }
        if starts(i, "ay") {
            out.push("E");
            out.push("j");
            i += 2;
            continue;
        } // R109 : pays ≈ /pEj/.
        // --- Lettres simples ---
        match c {
            'a' => {
                if at(i + 1) == 'î' {
                    out.push("E");
                    i += 2;
                }
                // R110 (filet de R082).
                else {
                    out.push("a");
                    i += 1;
                } // R111
            }
            'à' | 'â' => {
                out.push("a");
                i += 1;
            } // R112
            'e' => {
                // R113 : e final muet (table, rue) sauf monosyllabes.
                if end1
                    && n > 1
                    && word != "le"
                    && word != "de"
                    && word != "me"
                    && word != "te"
                    && word != "se"
                    && word != "ce"
                    && word != "je"
                    && word != "ne"
                    && word != "que"
                {
                    i += 1;
                    continue;
                }
                if word == "est" {
                    out.push("E");
                    i += 3;
                    continue;
                } // R114 : est → /E/.
                if word == "et" {
                    out.push("e");
                    i += 2;
                    continue;
                } // R115
                if starts(i, "et") && i + 2 >= n {
                    out.push("E");
                    i += 2;
                    continue;
                } // R116 : fouet.
                if starts(i, "ez") && i + 2 >= n {
                    out.push("e");
                    i += 2;
                    continue;
                } // R117 : parlez.
                if starts(i, "er") && i + 2 >= n {
                    // R118 : parler→/e/, mer→/mER/.
                    if er_sounds {
                        out.push("E");
                        out.push("R");
                    } else {
                        out.push("e");
                    }
                    i += 2;
                    continue;
                }
                if starts(i, "ed") && i + 2 >= n {
                    out.push("e");
                    i += 2;
                    continue;
                } // R119 : pied.
                if at(i + 1) == at(i + 2) && is_consonant_letter(at(i + 1)) {
                    out.push("E");
                    i += 1;
                    continue;
                } // R120 : belle.
                if at(i + 1) == 'r' && (i + 2 >= n || is_consonant_letter(at(i + 2))) {
                    out.push("E");
                    i += 1;
                    continue;
                } // R121 : merci, mer.
                if matches!(at(i + 1), 'r' | 'l' | 'c' | 'f') && i + 2 >= n {
                    out.push("E");
                    i += 1;
                    continue;
                } // R122 : sel, bec.
                if is_consonant_letter(at(i + 1)) && i + 2 >= n {
                    out.push("E");
                    i += 1;
                    continue;
                } // R123 : avec.
                if ["mes", "tes", "ses", "les", "des", "ces"].contains(&word.as_str()) {
                    out.push("e");
                    i += 1;
                    continue;
                } // R124
                if is_consonant_letter(at(i + 1)) && vowel_at(i + 2) {
                    out.push("@");
                    i += 1;
                    continue;
                } // R125 : schwa (demain).
                if end1 {
                    i += 1;
                    continue;
                } // R126 : e final de sécurité.
                out.push("@");
                i += 1; // R127 : schwa par défaut.
            }
            'é' => {
                out.push("e");
                i += 1;
            } // R128
            'è' | 'ê' | 'ë' => {
                out.push("E");
                i += 1;
            } // R129
            'i' => {
                if vowel_at(i + 1) {
                    out.push("j");
                    i += 1;
                    continue;
                } // R130 : ciel, pied.
                out.push("i");
                i += 1; // R131
            }
            'î' | 'ï' => {
                out.push("i");
                i += 1;
            } // R132 : maïs (diaérèse = hiatus).
            'o' => {
                if vowel_at(i + 1) {
                    out.push("O");
                    i += 1;
                    continue;
                } // R133 : poète, Noël.
                if at(i + 1) == at(i + 2) && is_consonant_letter(at(i + 1)) {
                    out.push("O");
                    i += 1;
                    continue;
                } // R134 : pomme.
                out.push("o");
                i += 1; // R135
            }
            'ô' => {
                out.push("o");
                i += 1;
            } // R136
            'u' => {
                if vowel_at(i + 1) {
                    out.push("hu");
                    i += 1;
                    continue;
                } // R137 : nuit, lui.
                out.push("y");
                i += 1; // R138 : rue.
            }
            'ù' | 'û' => {
                out.push("y");
                i += 1;
            } // R139
            'y' => {
                // R140 : /j/ au contact d'une voyelle, /i/ sinon.
                if vowel_at(i + 1) || (i > 0 && is_vowel_letter(w[i - 1])) {
                    out.push("j");
                } else {
                    out.push("i");
                }
                i += 1;
            }
            'œ' => {
                out.push("oe");
                i += 1;
            } // R141
            'æ' => {
                out.push("e");
                i += 1;
            } // R142
            'ç' => {
                out.push("s");
                i += 1;
            } // R143
            'c' => {
                if matches!(at(i + 1), 'e' | 'é' | 'è' | 'ê' | 'i' | 'î' | 'y') {
                    out.push("s");
                }
                // R144
                else {
                    out.push("k");
                } // R145 : café→/k/, sac→/k/.
                i += 1;
            }
            'g' => {
                if matches!(at(i + 1), 'e' | 'é' | 'è' | 'ê' | 'i' | 'î' | 'y') {
                    out.push("Z");
                }
                // R146 : girafe.
                else if end1 {
                    i += 1;
                    continue;
                }
                // R147 : g final muet (sang).
                else {
                    out.push("g");
                } // R148
                i += 1;
            }
            's' => {
                if end1 {
                    if s_sounds {
                        out.push("s");
                    } // R149 : fils, ours.
                    i += 1;
                    continue; // R150 : s final muet (temps, paris).
                }
                if i > 0 && is_vowel_letter(w[i - 1]) && vowel_at(i + 1) {
                    out.push("z");
                }
                // R151 : maison.
                else {
                    out.push("s");
                } // R152
                i += 1;
            }
            't' => {
                if end1 {
                    if i > 0 && is_consonant_letter(w[i - 1]) {
                        out.push("t");
                    } // R153 : ouest, contact.
                    i += 1;
                    continue; // R154 : t final muet après voyelle (chat).
                }
                out.push("t");
                i += 1; // R155
            }
            'd' => {
                if end1 && word != "sud" {
                    i += 1;
                    continue;
                } // R156 : d final muet (grand).
                out.push("d");
                i += 1; // R157
            }
            'p' => {
                if end1
                    && [
                        "coup", "beaucoup", "loup", "galop", "sirop", "trop", "drap", "galops",
                    ]
                    .contains(&word.as_str())
                {
                    i += 1;
                    continue; // R158 : p final muet.
                }
                out.push("p");
                i += 1; // R159
            }
            'b' => {
                if end1 && !b_sounds {
                    i += 1;
                    continue;
                } // R160 : plomb.
                out.push("b");
                i += 1; // R161
            }
            'r' => {
                if end1 && at(i - 1) == 'e' && word.ends_with("er") && !er_sounds && n > 2 {
                    i += 1;
                    continue; // R162 : -er infinitif/nom → r muet (parler).
                }
                out.push("R");
                i += 1; // R163
            }
            'l' => {
                if end1 && l_silent {
                    i += 1;
                    continue;
                } // R164 : outil, fusil.
                out.push("l");
                i += 1; // R165
            }
            'f' | 'v' | 'j' | 'k' | 'z' => {
                out.push(match c {
                    'f' => "f",
                    'v' => "v",
                    'j' => "Z",
                    'k' => "k",
                    _ => "z",
                }); // R166
                i += 1;
            }
            'q' => {
                out.push("k");
                i += 1;
            } // R167 : cinq (via inq → in + k).
            'w' => {
                out.push("v");
                i += 1;
            } // R168 : wagon → /v/ (FR).
            'm' | 'n' => {
                // R169 : nasale déjà traitée → consonne ; finale après nasale → muette.
                if end1
                    && i >= 2
                    && matches!(w[i - 2], 'a' | 'e' | 'o')
                    && matches!(w[i - 1], 'n' | 'm')
                {
                    i += 1;
                    continue;
                }
                out.push(if c == 'm' { "m" } else { "n" });
                i += 1;
            }
            'x' => {
                if i == 0 && vowel_at(1) {
                    out.push("g");
                    out.push("z"); // R169 : xavier.
                } else if end1 {
                    i += 1;
                    continue; // R170 : x muet final (paix, deux).
                } else {
                    // R171 : x → /ks/ (extra, axer, fixe).
                    out.push("k");
                    out.push("s");
                }
                i += 1;
            }
            'h' => {
                i += 1;
            } // R174 : h toujours muet (FR).
            _ => {
                i += 1;
            } // R175 : caractère non géré → ignoré (documenté).
        }
    }
}

/// G2P anglais compact (~35 règles, E001…).
fn g2p_en_word(w: &[char], out: &mut Vec<&'static str>) {
    let word: String = w.iter().collect();
    // Règles lexicales EN.
    match word.as_str() {
        // E001 : mots-outils.
        "the" => {
            out.extend(["dh", "@"]);
            return;
        }
        "this" => {
            out.extend(["dh", "I", "s"]);
            return;
        }
        "that" => {
            out.extend(["dh", "ae", "t"]);
            return;
        }
        "these" => {
            out.extend(["dh", "i", "z"]);
            return;
        }
        "those" => {
            out.extend(["dh", "o", "z"]);
            return;
        }
        "a" => {
            out.push("@");
            return;
        }
        "of" => {
            out.extend(["A", "v"]);
            return;
        }
        "and" => {
            out.extend(["ae", "n", "d"]);
            return;
        }
        _ => {}
    }
    let n = w.len();
    let mut i = 0;
    let at = |j: usize| -> char { if j < n { w[j] } else { '\0' } };
    let starts = |j: usize, s: &str| -> bool {
        let v: Vec<char> = s.chars().collect();
        j + v.len() <= n && w[j..j + v.len()] == v[..]
    };
    let vowel_at = |j: usize| -> bool { j < n && is_vowel_letter(w[j]) };
    while i < n {
        if starts(i, "tion") || starts(i, "sion") {
            out.extend(["S", "@", "n"]);
            i += 4;
            continue;
        } // E010
        if starts(i, "tch") {
            out.push("tS");
            i += 3;
            continue;
        } // E011
        if starts(i, "igh") {
            out.push("a");
            out.push("j");
            i += 3;
            continue;
        } // E012 : night ≈ /najt/.
        if starts(i, "ight") {
            out.extend(["a", "j", "t"]);
            i += 4;
            continue;
        } // E013
        if starts(i, "ing") && i + 3 >= n {
            out.extend(["I", "ng"]);
            i += 3;
            continue;
        } // E014
        if starts(i, "oo") {
            out.push("u");
            i += 2;
            continue;
        } // E015
        if starts(i, "ee") {
            out.push("i");
            i += 2;
            continue;
        } // E016
        if starts(i, "ea") {
            out.push("i");
            i += 2;
            continue;
        } // E017 : beach (≈, bread approximé).
        if starts(i, "oa") {
            out.push("o");
            i += 2;
            continue;
        } // E018
        if starts(i, "ow") && i + 2 >= n {
            out.push("o");
            i += 2;
            continue;
        } // E019 : window.
        if starts(i, "ow") {
            out.push("a");
            out.push("w");
            i += 2;
            continue;
        } // E020 : how.
        if starts(i, "ou") {
            out.push("a");
            out.push("w");
            i += 2;
            continue;
        } // E021 : house (robotique).
        if starts(i, "th") {
            // E022 : /ð/ pour mots-outils, /θ/ sinon.
            if [
                "this", "that", "these", "those", "the", "them", "then", "than", "though",
            ]
            .iter()
            .any(|m| word == *m || word.starts_with(&format!("{}e", m)))
            {
                out.push("dh");
            } else {
                out.push("th");
            }
            i += 2;
            continue;
        }
        if starts(i, "sh") {
            out.push("S");
            i += 2;
            continue;
        } // E023
        if starts(i, "ch") {
            out.push("tS");
            i += 2;
            continue;
        } // E024
        if starts(i, "wh") {
            out.push("w");
            i += 2;
            continue;
        } // E025
        if starts(i, "ph") {
            out.push("f");
            i += 2;
            continue;
        } // E026
        if starts(i, "ck") {
            out.push("k");
            i += 2;
            continue;
        } // E027
        if starts(i, "kn") {
            out.push("n");
            i += 2;
            continue;
        } // E028 : know.
        if starts(i, "wr") {
            out.push("r");
            i += 2;
            continue;
        } // E029 : write (r≈R).
        if starts(i, "gh") {
            i += 2;
            continue;
        } // E030 : night (déjà igh, filet).
        if starts(i, "ed") && i + 2 >= n {
            out.push("d");
            i += 2;
            continue;
        } // E031 : played ≈ /plejd/.
        if starts(i, "ng") {
            out.push("ng");
            i += 2;
            continue;
        } // E032
        if starts(i, "qu") {
            out.push("k");
            out.push("w");
            i += 2;
            continue;
        } // E033
        match w[i] {
            'a' => {
                out.push("ae");
                i += 1;
            } // E034
            'e' => {
                if i + 1 >= n {
                    i += 1;
                    continue;
                } // E035 : e final muet EN.
                out.push("E");
                i += 1; // E036
            }
            'i' => {
                if vowel_at(i + 1) {
                    out.push("j");
                }
                // E037
                else {
                    out.push("I");
                } // E038
                i += 1;
            }
            'o' => {
                out.push("O");
                i += 1;
            } // E039
            'u' => {
                out.push("A");
                i += 1;
            } // E040
            'y' => {
                if i + 1 >= n {
                    out.push("i");
                }
                // E041 : happy.
                else if vowel_at(i + 1) {
                    out.push("j");
                }
                // E042
                else {
                    out.push("I");
                } // E043
                i += 1;
            }
            'c' => {
                if matches!(at(i + 1), 'e' | 'i' | 'y') {
                    out.push("s");
                }
                // E044
                else {
                    out.push("k");
                } // E045
                i += 1;
            }
            'g' => {
                if matches!(at(i + 1), 'e' | 'i' | 'y') {
                    out.push("Z");
                }
                // E046 (≈ /dʒ/ simplifié).
                else {
                    out.push("g");
                } // E047
                i += 1;
            }
            'h' => {
                if i == 0 {
                    out.push("h");
                }
                i += 1;
            } // E048 : h aspiré (EN).
            'j' => {
                out.push("d");
                out.push("Z");
                i += 1;
            } // E049 : job ≈ /dZ/.
            'r' => {
                out.push("R");
                i += 1;
            } // E050 : rhotique ≈ /R/ (robotique assumé).
            's' => {
                out.push("s");
                i += 1;
            }
            't' => {
                out.push("t");
                i += 1;
            }
            'd' => {
                out.push("d");
                i += 1;
            }
            'p' => {
                out.push("p");
                i += 1;
            }
            'b' => {
                out.push("b");
                i += 1;
            }
            'f' => {
                out.push("f");
                i += 1;
            }
            'v' => {
                out.push("v");
                i += 1;
            }
            'k' => {
                out.push("k");
                i += 1;
            }
            'l' => {
                out.push("l");
                i += 1;
            }
            'm' => {
                out.push("m");
                i += 1;
            }
            'n' => {
                out.push("n");
                i += 1;
            }
            'w' => {
                out.push("w");
                i += 1;
            }
            'x' => {
                out.push("k");
                out.push("s");
                i += 1;
            }
            'z' => {
                out.push("z");
                i += 1;
            }
            'q' => {
                out.push("k");
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Prosodie + rendu
// ---------------------------------------------------------------------------

struct TimedPh {
    sym: &'static str,
    start: usize,
    len: usize,
    f0_start: f32,
    f0_end: f32,
    /// Gain local (accentuation des majuscules R005 ; 1.0 sinon).
    amp: f32,
}

fn is_pause_sym(sym: &str) -> bool {
    sym.starts_with("pau")
}

fn pause_ms(sym: &str) -> f32 {
    match sym {
        "pau30" => 30.0,
        "pau90" => 90.0,
        "pau180" => 180.0,
        _ => 30.0,
    }
}

/// Construit la timeline (échantillons) + courbe F0 (déclinaison, accents).
fn build_timeline(syms: &[&'static str], voice: &FormantVoice) -> Vec<TimedPh> {
    let sr = FORMANT_SAMPLE_RATE as f32;
    // Découpe en phrases (reset déclinaison après .!?).
    let mut tl: Vec<TimedPh> = Vec::new();
    let mut t = 0usize;
    let mut sent_start_idx = 0usize;
    let mut boost = 1.0f32;
    let mut rise_tail = false;
    // R005 : accentuation modérée du mot en cours (majuscule).
    let mut cap_active = false;
    let mut idx = 0;
    while idx < syms.len() {
        let s = syms[idx];
        if s == CAP_MARK {
            cap_active = true;
            idx += 1;
            continue;
        }
        if s == "rise?" || s == "boost!" || s == "fall." {
            // Marqueur prosodique : s'applique à la phrase en cours.
            match s {
                "rise?" => rise_tail = true,
                "boost!" => boost = 1.12,
                _ => {}
            }
            idx += 1;
            continue;
        }
        if is_pause_sym(s) && (s == "pau180") {
            // Fin de phrase : fige la F0 de fin (chute) puis reset.
            let total: usize = tl[sent_start_idx..].iter().map(|p| p.len).sum();
            let s0 = tl.get(sent_start_idx).map(|p| p.start).unwrap_or(0);
            for p in tl[sent_start_idx..].iter_mut() {
                let prog = if total == 0 {
                    0.0
                } else {
                    (p.start - s0) as f32 / total as f32
                };
                let decl = 1.0 - 0.12 * prog;
                p.f0_start = voice.f0_base_hz * decl * boost;
                p.f0_end = voice.f0_base_hz * decl * boost;
            }
            if rise_tail {
                // Montée interrogative sur les 2 derniers phonèmes voisés.
                let mut count = 0;
                for p in tl[sent_start_idx..].iter_mut().rev() {
                    if is_pause_sym(p.sym) {
                        continue;
                    }
                    if is_voiced_sym(p.sym) && count < 2 {
                        p.f0_end = voice.f0_base_hz * 1.30 * boost;
                        p.f0_start = (p.f0_start + p.f0_end) / 2.0;
                        count += 1;
                    }
                }
            } else {
                // Chute finale déclarative sur le dernier voisé.
                for p in tl[sent_start_idx..].iter_mut().rev() {
                    if !is_pause_sym(p.sym) && is_voiced_sym(p.sym) {
                        p.f0_end = voice.f0_base_hz * 0.85 * boost;
                        break;
                    }
                }
            }
            // Pause elle-même.
            let len = (pause_ms(s) / 1000.0 * sr / voice.rate) as usize;
            tl.push(TimedPh {
                sym: "pau",
                start: t,
                len,
                f0_start: 0.0,
                f0_end: 0.0,
                amp: 1.0,
            });
            t += len;
            sent_start_idx = tl.len();
            boost = 1.0;
            rise_tail = false;
            cap_active = false;
            idx += 1;
            continue;
        }
        // Phonème normal : durée adaptée (accent = dernière voyelle du mot
        // approximé par allongement si suivi d'une micro-pause).
        let p = phoneme_params(s);
        let is_pause = is_pause_sym(s);
        // R005 : léger allongement + énergie sur les mots accentués.
        let cap = cap_active && !is_pause;
        let next_is_pause = idx + 1 < syms.len() && is_pause_sym(syms[idx + 1]);
        let mut dur = p.dur_ms;
        if next_is_pause && p.voiced && p.noise_gain < 0.5 {
            dur *= 1.25; // Accent final FR (allongement + F0, voir ci-dessous).
        }
        if cap {
            dur *= 1.06;
        }
        let len = ((dur / 1000.0 * sr) / voice.rate) as usize;
        let accent = if next_is_pause && p.voiced { 1.10 } else { 1.0 };
        let accent = if cap && p.voiced {
            accent * 1.04
        } else {
            accent
        };
        tl.push(TimedPh {
            sym: s,
            start: t,
            len: len.max(24),
            f0_start: voice.f0_base_hz * accent * boost,
            f0_end: voice.f0_base_hz * accent * boost,
            amp: if cap { 1.06 } else { 1.0 },
        });
        t += len.max(24);
        if is_pause {
            cap_active = false;
        }
        idx += 1;
    }
    // Déclinaison sur la dernière phrase (sans ponctuation finale).
    if sent_start_idx < tl.len() {
        let total: usize = tl[sent_start_idx..].iter().map(|p| p.len).sum();
        let s0 = tl[sent_start_idx].start;
        for p in tl[sent_start_idx..].iter_mut() {
            if is_pause_sym(p.sym) {
                continue;
            }
            let prog = if total == 0 {
                0.0
            } else {
                (p.start - s0) as f32 / total as f32
            };
            let decl = 1.0 - 0.12 * prog;
            p.f0_start *= decl;
            p.f0_end *= decl;
        }
    }
    tl
}

struct Resonator {
    y1: f32,
    y2: f32,
}

impl Resonator {
    fn step(&mut self, x: f32, f: f32, bw: f32) -> f32 {
        // Résonateur Klatt (1980) : pôles en exp(-π·BW·T)·exp(±j2π·F·T),
        // toujours stables (|pôles| < 1). C = -exp(-2π·BW·T) (SIGNED !).
        let t = 1.0 / FORMANT_SAMPLE_RATE as f32;
        let c = -(-2.0 * std::f32::consts::PI * bw * t).exp();
        let b = 2.0
            * (-std::f32::consts::PI * bw * t).exp()
            * (2.0 * std::f32::consts::PI * f * t).cos();
        let a = 1.0 - b - c;
        let y = a * x + b * self.y1 + c * self.y2;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

struct SynthState {
    res: [Resonator; 5],
    cur_f: [f32; 5],
    phase: f32,     // phase glottique [0,1).
    prev_flow: f32, // différentiation du flux glottique.
    noise: u64,     // xorshift.
    lp_noise: f32,  // souffle passe-bas.
    init: bool,
}

impl SynthState {
    fn new() -> Self {
        Self {
            res: [
                Resonator { y1: 0.0, y2: 0.0 },
                Resonator { y1: 0.0, y2: 0.0 },
                Resonator { y1: 0.0, y2: 0.0 },
                Resonator { y1: 0.0, y2: 0.0 },
                Resonator { y1: 0.0, y2: 0.0 },
            ],
            cur_f: [500.0, 1500.0, 2500.0, 3500.0, 4500.0],
            phase: 0.0,
            prev_flow: 0.0,
            noise: 0x2545F4914F6CDD1D,
            lp_noise: 0.0,
            init: false,
        }
    }

    fn white(&mut self) -> f32 {
        self.noise ^= self.noise << 13;
        self.noise ^= self.noise >> 7;
        self.noise ^= self.noise << 17;
        ((self.noise >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0) as f32
    }
}

/// Pulse glottique de Rosenberg simplifié (phase [0,1)).
fn glottal_flow(phase: f32) -> f32 {
    if phase < 0.6 {
        (std::f32::consts::PI * phase / 0.6).sin().powi(2)
    } else {
        -0.15 * (std::f32::consts::PI * (phase - 0.6) / 0.4).sin()
    }
}

/// Moteur de rendu avec continuité d'état (utilisé en one-shot et en stream).
pub struct FormantStream {
    tl: Vec<TimedPh>,
    total: usize,
    pos: usize,
    state: SynthState,
    voice: FormantVoice,
}

impl FormantStream {
    pub fn new(text: &str, voice: &FormantVoice) -> Self {
        // Phonémisation interne : conserve le marqueur d'accentuation (R005).
        let syms = phonemize_with_prosody(text);
        let tl = build_timeline(&syms, voice);
        let total = tl.iter().map(|p| p.start + p.len).max().unwrap_or(0);
        Self {
            tl,
            total,
            pos: 0,
            state: SynthState::new(),
            voice: voice.clone(),
        }
    }

    pub fn is_done(&self) -> bool {
        self.pos >= self.total || self.total == 0
    }

    pub fn total_samples(&self) -> usize {
        self.total
    }

    /// Timeline de visèmes dérivée de la timeline phonémique RÉELLE.
    ///
    /// Les visèmes sont **phonémiques** (coarticulation non modélisée) : la
    /// timeline est donc cohérente PAR CONSTRUCTION avec l'audio synthétisé
    /// (même source), et l'invariant « somme des durées == total_samples »
    /// est vérifié par test.
    pub fn viseme_timeline(&self) -> Vec<crate::viseme::VisemeFrame> {
        self.tl
            .iter()
            .map(|p| crate::viseme::VisemeFrame {
                viseme: crate::viseme::viseme_for_phoneme(p.sym),
                start_sample: p.start,
                len_samples: p.len,
            })
            .collect()
    }

    fn phoneme_at(&self, pos: usize) -> Option<&TimedPh> {
        // Recherche dichotomique sur les débuts.
        let mut lo = 0usize;
        let mut hi = self.tl.len();
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.tl[mid].start + self.tl[mid].len <= pos {
                lo = mid + 1;
            } else if self.tl[mid].start > pos {
                hi = mid;
            } else {
                return Some(&self.tl[mid]);
            }
        }
        None
    }

    /// Rend jusqu'à `n` échantillons avec continuité de phase et de filtres.
    pub fn next_chunk(&mut self, n: usize) -> Vec<f32> {
        let mut out = Vec::new();
        if self.total == 0 {
            return out;
        }
        let target = (self.pos + n).min(self.total);
        // Lissage formantique : constante ~6 ms.
        let smooth = 1.0 - (-1.0 / (0.006 * FORMANT_SAMPLE_RATE as f32)).exp();
        while self.pos < target {
            let (formants, bw_scale, voiced, noise_gain, f0, amp) = match self.phoneme_at(self.pos)
            {
                Some(p) => {
                    let prm = phoneme_params(p.sym);
                    let frac = if p.len == 0 {
                        0.0
                    } else {
                        (self.pos - p.start) as f32 / p.len as f32
                    };
                    let f0 = p.f0_start + (p.f0_end - p.f0_start) * frac.clamp(0.0, 1.0);
                    (
                        prm.formants,
                        prm.bw_scale,
                        prm.voiced,
                        prm.noise_gain,
                        f0,
                        p.amp,
                    )
                }
                None => (
                    [500.0, 1500.0, 2500.0, 3500.0, 4500.0],
                    1.0,
                    false,
                    0.0,
                    0.0,
                    1.0,
                ),
            };
            if !self.state.init {
                self.state.cur_f = formants;
                self.state.init = true;
            }
            for (cur, &f) in self.state.cur_f.iter_mut().zip(formants.iter()) {
                *cur += (f - *cur) * smooth;
            }
            // Source.
            let src = if voiced {
                let f0c = f0.clamp(FORMANT_MIN_F0, FORMANT_MAX_F0);
                self.state.phase += f0c / FORMANT_SAMPLE_RATE as f32;
                if self.state.phase >= 1.0 {
                    self.state.phase -= 1.0;
                }
                let flow = glottal_flow(self.state.phase);
                let diff = flow - self.state.prev_flow; // différenciation → buzz réaliste.
                self.state.prev_flow = flow;
                let asp = self.state.white() * self.voice.breathiness * 0.35;
                self.state.lp_noise += 0.25 * (asp - self.state.lp_noise);
                diff * 2.2 + self.state.lp_noise + self.state.white() * noise_gain * 0.4
            } else if noise_gain > 0.0 {
                self.state.white() * noise_gain
            } else {
                0.0
            };
            // Cascade de 5 résonateurs.
            let mut y = src;
            for ((r, &f), &bw) in self
                .state
                .res
                .iter_mut()
                .zip(self.state.cur_f.iter())
                .zip(FORMANT_BW.iter())
            {
                y = r.step(y, f, bw * bw_scale);
            }
            // Saturation douce anti-clic + gain voix + accent local (R005).
            out.push((y * 0.9).tanh() * self.voice.gain * amp);
            self.pos += 1;
        }
        // Fondu de 5 ms en début/fin d'énoncé (anti-clic global).
        let fade = (0.005 * FORMANT_SAMPLE_RATE as f32) as usize;
        let out_len = out.len();
        let end_pos = self.pos;
        for (k, s) in out.iter_mut().enumerate() {
            let global = end_pos - out_len + k;
            let a = (global.min(fade) as f32 / fade.max(1) as f32).min(1.0);
            let b = ((self.total.saturating_sub(global)).min(fade) as f32 / fade.max(1) as f32)
                .min(1.0);
            *s *= a.min(b);
        }
        out
    }
}

/// Synthèse one-shot : texte → 24 kHz `f32` (vide si aucun phonème).
pub fn synthesize(text: &str, voice: &FormantVoice) -> Vec<f32> {
    let mut stream = FormantStream::new(text, voice);
    stream.next_chunk(stream.total_samples())
}

/// Estime F0 par autocorrélation normalisée (diagnostic/preuve, 70–320 Hz).
pub fn estimate_f0_autocorr(samples: &[f32], sample_rate: u32) -> Option<f32> {
    if samples.len() < sample_rate as usize / 4 {
        return None;
    }
    let seg: Vec<f32> = {
        let mid = samples.len() / 2;
        let half = (sample_rate as usize / 8).min(mid);
        samples[mid - half..mid + half].to_vec()
    };
    let mean: f32 = seg.iter().sum::<f32>() / seg.len() as f32;
    let v: Vec<f32> = seg.iter().map(|x| x - mean).collect();
    let e0: f32 = v.iter().map(|x| x * x).sum();
    if e0 < 1e-9 {
        return None;
    }
    let min_lag = (sample_rate as f32 / 320.0) as usize;
    let max_lag = (sample_rate as f32 / 70.0) as usize;
    let mut best_lag = min_lag;
    let mut best_r = f32::NEG_INFINITY;
    for lag in min_lag..max_lag.min(v.len()) {
        let r: f32 = v[..v.len() - lag]
            .iter()
            .zip(&v[lag..])
            .map(|(a, b)| a * b)
            .sum::<f32>()
            / e0;
        if r > best_r {
            best_r = r;
            best_lag = lag;
        }
    }
    if best_r < 0.25 {
        return None; // Pas de périodicité fiable.
    }
    Some(sample_rate as f32 / best_lag as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_viseme_timeline_is_consistent_with_the_synthesized_audio() {
        // INVARIANT FORT : la timeline de visèmes couvre EXACTEMENT l'audio
        // produit (pas de dérive possible entre visèmes et son).
        let v = FormantVoice::default();
        let text = "Bonjour, voici votre synthèse du jour.";
        let stream = FormantStream::new(text, &v);
        let timeline = stream.viseme_timeline();
        assert!(!timeline.is_empty(), "une timeline est produite");
        let sum: usize = timeline.iter().map(|f| f.len_samples).sum();
        assert_eq!(
            sum,
            stream.total_samples(),
            "la somme des durées de visèmes == durée totale de l'audio"
        );
        // Les visèmes ne sont jamais « inconnus » pour un texte réel.
        assert!(
            timeline
                .iter()
                .all(|f| f.viseme != crate::viseme::Viseme::Unknown),
            "aucun phonème réel ne doit être non cartographié"
        );
        // Un texte vide ne produit AUCUNE timeline (pas de visème inventé).
        let vide = FormantStream::new("", &v);
        assert!(vide.viseme_timeline().is_empty());
        assert_eq!(vide.total_samples(), 0);
    }

    #[test]
    fn test_duree_proportionnelle_aux_phonemes() {
        let v = FormantVoice::default();
        let court = synthesize("a", &v);
        let long = synthesize("bonjour madame comment allez vous", &v);
        assert!(!court.is_empty() && !long.is_empty());
        let ratio = long.len() as f32 / court.len() as f32;
        assert!(ratio > 5.0, "ratio={}", ratio);
    }

    #[test]
    fn test_energie_positive_et_texte_vide() {
        let v = FormantVoice::default();
        let s = synthesize("bonjour", &v);
        let e: f32 = s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32;
        assert!(e > 1e-6, "énergie={}", e);
        assert!(s.iter().all(|x| x.is_finite()));
        assert!(synthesize("", &v).is_empty());
        assert!(synthesize("   ... ", &v).iter().all(|x| x.is_finite()));
    }

    #[test]
    fn test_f0_mesuree_dans_la_plage_demandee() {
        for f0 in [100.0f32, 120.0, 200.0] {
            let v = FormantVoice::new(f0, 1.0);
            let s = synthesize("aaa", &v);
            let mes = estimate_f0_autocorr(&s, FORMANT_SAMPLE_RATE).expect("F0 mesurable");
            let err = (mes - f0).abs() / f0;
            assert!(err < 0.15, "f0 demandée={} mesurée={}", f0, mes);
            eprintln!("F0 demandée={} mesurée={:.1}", f0, mes);
        }
    }

    #[test]
    fn test_bonjour_different_de_au_revoir() {
        let v = FormantVoice::default();
        let a = synthesize("bonjour", &v);
        let b = synthesize("au revoir", &v);
        let m = a.len().min(b.len());
        assert!(m > 1000);
        let num: f32 = a[..m]
            .iter()
            .zip(&b[..m])
            .map(|(x, y)| (x - y) * (x - y))
            .sum();
        let den: f32 =
            a[..m].iter().map(|x| x * x).sum::<f32>() + b[..m].iter().map(|y| y * y).sum::<f32>();
        let dist = (num / den.max(1e-9)).sqrt();
        assert!(dist > 0.35, "distance normalisée={}", dist);
        eprintln!(
            "bonjour vs au revoir: dist={:.3} len={}/{}",
            dist,
            a.len(),
            b.len()
        );
    }

    #[test]
    fn test_determinisme_et_phonemisation_saine() {
        let v = FormantVoice::default();
        let a = synthesize("bonjour", &v);
        let b = synthesize("bonjour", &v);
        assert_eq!(a, b);
        let ph = phonemize("bonjour");
        assert!(ph.len() >= 5, "{:?}", ph);
        assert!(ph.contains(&"on"), "nasale attendue: {:?}", ph);
        let chat = phonemize("chat");
        assert!(chat.contains(&"S"), "ch→/S/: {:?}", chat);
        // ~100 règles : la couverture FR/EN de base doit tenir.
        assert!(phonemize("gnome").contains(&"g"));
        assert!(phonemize("montagne").contains(&"gn"));
        assert!(phonemize("hello").contains(&"h"));
        assert!(phonemize("table").len() >= 3);
    }

    #[test]
    fn test_stream_continuite_identique_au_oneshot() {
        let v = FormantVoice::default();
        let text = "bonjour madame";
        let one = synthesize(text, &v);
        let mut st = FormantStream::new(text, &v);
        let mut chunks = Vec::new();
        while !st.is_done() {
            chunks.extend(st.next_chunk(6000));
        }
        assert_eq!(chunks.len(), one.len());
        let maxd: f32 = chunks
            .iter()
            .zip(one.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(maxd < 1e-6, "écart max={}", maxd);
    }

    #[test]
    fn test_pauses_ponctuation_toutes_presentes() {
        assert!(phonemize("un,").contains(&"pau90"));
        assert!(phonemize("un;").contains(&"pau90"));
        assert!(phonemize("un:").contains(&"pau90"));
        assert!(phonemize("un.").contains(&"pau180"));
        assert!(phonemize("un!").contains(&"pau180"));
        assert!(phonemize("un?").contains(&"pau180"));
        assert!(phonemize("un?").contains(&"rise?"));
        assert!(phonemize("un!").contains(&"boost!"));
        // Toutes les pauses sont bien plus longues qu'une micro-pause inter-mot.
        assert!(pause_ms("pau90") > pause_ms("pau30"));
        assert!(pause_ms("pau180") > pause_ms("pau90"));
    }

    #[test]
    fn test_majuscules_accentuees_sans_changer_phonemize() {
        let v = FormantVoice::default();
        let lower = synthesize("bonjour", &v);
        let upper = synthesize("Bonjour", &v);
        assert!(!lower.is_empty() && !upper.is_empty());
        assert!(
            upper.len() > lower.len(),
            "majuscule accentuée : {} doit dépasser {}",
            upper.len(),
            lower.len()
        );
        // Le même mot reste phonémisé identiquement (marqueur filtré).
        assert_eq!(phonemize("Bonjour"), phonemize("bonjour"));
        assert_eq!(phonemize("MADAME"), phonemize("madame"));
        // Déterminisme conservé avec accentuation.
        assert_eq!(synthesize("Bonjour.", &v), synthesize("Bonjour.", &v));
    }
}
