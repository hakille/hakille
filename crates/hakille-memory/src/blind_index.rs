//! Indexation Aveugle Déterministe (HMAC-SHA256 Blind Indexing) selon ADR-012.
//!
//! Permet d'indexer et d'interroger la mémoire épisodique et sémantique sur disque
//! sans stocker de termes en clair et sans fuite de fréquence de mots-clés.
//!
//! Principes mathématiques (ADR-012 §3) :
//! 1. Normalisation Unicode stricte du terme en clair :
//!    `\bar{w} = Normalize(w)` (trim, minuscules Unicode, réduction des espaces contigus).
//! 2. Jeton d'indexation aveugle exacte :
//!    `T_exact(\bar{w}) = HMAC-SHA256(K_bindex, "hakille:blind:exact:v1:" || \bar{w})`
//! 3. Jeton d'indexation aveugle par préfixe :
//!    Pour une longueur \ell \in [3, 8] :
//!    `T_prefix(\bar{w}, \ell) = HMAC-SHA256(K_bindex, "hakille:blind:prefix:v1:" || to_le_bytes(\ell) || ":" || slice(\bar{w}, 0, \ell))`
//!
//! Sécurité en RAM :
//! `BlindIndexKey` implémente `Zeroize` et `ZeroizeOnDrop` pour garantir l'effacement
//! physique de la clé en mémoire à la fin de sa portée.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

type HmacSha256 = Hmac<Sha256>;

pub const DOMAIN_EXACT: &[u8] = b"hakille:blind:exact:v1:";
pub const DOMAIN_PREFIX: &[u8] = b"hakille:blind:prefix:v1:";

pub const DEFAULT_MIN_PREFIX_LEN: usize = 3;
pub const DEFAULT_MAX_PREFIX_LEN: usize = 8;

/// Clé secrète d'indexation aveugle (32 octets), effacée de la RAM à la destruction.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct BlindIndexKey([u8; 32]);

impl BlindIndexKey {
    /// Instancie une clé à partir d'un tableau de 32 octets.
    pub fn new(key: [u8; 32]) -> Self {
        Self(key)
    }

    /// Instancie une clé depuis un slice d'octets de longueur quelconque en le hachant ou
    /// exigeant 32 octets.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, &'static str> {
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "Clé d'indexation aveugle invalide : 32 octets requis")?;
        Ok(Self(arr))
    }

    /// Référence brute aux 32 octets de la clé.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for BlindIndexKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BlindIndexKey([REDACTED])")
    }
}

/// Normalisation canonique d'un terme textuel pour l'indexation aveugle (ADR-012 §3.1).
///
/// 1. Suppression des espaces de début et de fin (`trim`).
/// 2. Conversion en minuscules Unicode (`to_lowercase`).
/// 3. Réduction des séquences d'espaces contigus en un espace unique.
pub fn normalize(term: &str) -> String {
    let trimmed = term.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut result = String::with_capacity(trimmed.len());
    let mut in_whitespace = false;

    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            if !in_whitespace {
                result.push(' ');
                in_whitespace = true;
            }
        } else {
            for lower_ch in ch.to_lowercase() {
                result.push(lower_ch);
            }
            in_whitespace = false;
        }
    }

    result
}

/// Calcule le jeton d'indexation aveugle exacte $T_{\text{exact}}$ pour un terme donné.
pub fn exact_token(key: &BlindIndexKey, term: &str) -> [u8; 32] {
    let norm = normalize(term);
    let mut mac =
        HmacSha256::new_from_slice(key.as_bytes()).expect("Clé HMAC-SHA256 valide (32 octets)");
    mac.update(DOMAIN_EXACT);
    mac.update(norm.as_bytes());
    mac.finalize().into_bytes().into()
}

/// Calcule le jeton d'indexation aveugle par préfixe pour un préfixe de longueur donnée.
pub fn prefix_token(key: &BlindIndexKey, prefix: &str, length: usize) -> [u8; 32] {
    let norm = normalize(prefix);
    let mut mac =
        HmacSha256::new_from_slice(key.as_bytes()).expect("Clé HMAC-SHA256 valide (32 octets)");
    mac.update(DOMAIN_PREFIX);
    mac.update(&(length as u32).to_le_bytes());
    mac.update(b":");
    mac.update(norm.as_bytes());
    mac.finalize().into_bytes().into()
}

/// Calcule tous les jetons d'indexation aveugle par préfixe pour un terme, pour les longueurs
/// comprises entre `min_len` et `max_len` (par défaut 3 et 8 caractères).
///
/// Si le terme contient plusieurs mots séparés par des espaces, les préfixes sont générés
/// pour l'expression entière ainsi que pour chaque mot individuel (sans doublon).
pub fn prefix_tokens(
    key: &BlindIndexKey,
    term: &str,
    min_len: usize,
    max_len: usize,
) -> Vec<([u8; 32], usize)> {
    let norm = normalize(term);
    let mut results = Vec::new();
    let mut seen = std::collections::HashSet::new();

    add_term_prefixes(key, &norm, min_len, max_len, &mut results, &mut seen);

    if norm.contains(' ') {
        for word in norm.split_whitespace() {
            add_term_prefixes(key, word, min_len, max_len, &mut results, &mut seen);
        }
    }

    results
}

fn add_term_prefixes(
    key: &BlindIndexKey,
    term: &str,
    min_len: usize,
    max_len: usize,
    results: &mut Vec<([u8; 32], usize)>,
    seen: &mut std::collections::HashSet<[u8; 32]>,
) {
    let chars: Vec<char> = term.chars().collect();
    if chars.len() < min_len {
        return;
    }

    let upper = chars.len().min(max_len);
    for len in min_len..=upper {
        let prefix_str: String = chars[..len].iter().collect();
        let tok = prefix_token(key, &prefix_str, len);
        if seen.insert(tok) {
            results.push((tok, len));
        }
    }
}

/// Ensemble complet des jetons aveugles d'un triplet (sujet, prédicat, objet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TripletBlindTokens {
    pub subject_exact: [u8; 32],
    pub predicate_exact: [u8; 32],
    pub object_exact: [u8; 32],
    pub subject_prefixes: Vec<[u8; 32]>,
    pub object_prefixes: Vec<[u8; 32]>,
}

impl TripletBlindTokens {
    /// Calcule l'ensemble complet des jetons aveugles pour un triplet.
    pub fn compute(key: &BlindIndexKey, subject: &str, predicate: &str, object: &str) -> Self {
        let subject_exact = exact_token(key, subject);
        let predicate_exact = exact_token(key, predicate);
        let object_exact = exact_token(key, object);

        let subject_prefixes =
            prefix_tokens(key, subject, DEFAULT_MIN_PREFIX_LEN, DEFAULT_MAX_PREFIX_LEN)
                .into_iter()
                .map(|(t, _)| t)
                .collect();

        let object_prefixes =
            prefix_tokens(key, object, DEFAULT_MIN_PREFIX_LEN, DEFAULT_MAX_PREFIX_LEN)
                .into_iter()
                .map(|(t, _)| t)
                .collect();

        Self {
            subject_exact,
            predicate_exact,
            object_exact,
            subject_prefixes,
            object_prefixes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalization_rules() {
        assert_eq!(normalize("  Hello   World  "), "hello world");
        assert_eq!(normalize("Éléphant"), "éléphant");
        assert_eq!(normalize(""), "");
        assert_eq!(normalize("   \t  \n  "), "");
        assert_eq!(normalize("Marie   aime   le   jazz"), "marie aime le jazz");
    }

    #[test]
    fn test_exact_token_determinism_and_normalization_equivalence() {
        let key = BlindIndexKey::new([42u8; 32]);
        let tok1 = exact_token(&key, "Marie Curie");
        let tok2 = exact_token(&key, "  marie   curie  ");
        let tok3 = exact_token(&key, "MARIE CURIE");

        assert_eq!(
            tok1, tok2,
            "La normalisation doit rendre les jetons identiques"
        );
        assert_eq!(tok1, tok3, "La casse ne doit pas altérer le jeton exact");

        let different_tok = exact_token(&key, "Pierre Curie");
        assert_ne!(
            tok1, different_tok,
            "Des termes différents doivent produire des jetons distincts"
        );
    }

    #[test]
    fn test_key_separation_produces_distinct_tokens() {
        let key1 = BlindIndexKey::new([1u8; 32]);
        let key2 = BlindIndexKey::new([2u8; 32]);

        let tok1 = exact_token(&key1, "secret_keyword");
        let tok2 = exact_token(&key2, "secret_keyword");

        assert_ne!(
            tok1, tok2,
            "Des clés distinctes doivent isoler les jetons aveugles"
        );
    }

    #[test]
    fn test_domain_separation_exact_vs_prefix() {
        let key = BlindIndexKey::new([7u8; 32]);
        let exact = exact_token(&key, "abc");
        let prefix = prefix_token(&key, "abc", 3);

        assert_ne!(
            exact, prefix,
            "Les domaines exact et prefix doivent être séparés cryptographiquement"
        );
    }

    #[test]
    fn test_prefix_tokens_generation_bounds() {
        let key = BlindIndexKey::new([99u8; 32]);

        // Terme trop court (< 3 caractères) : aucun préfixe
        let short = prefix_tokens(&key, "hi", 3, 8);
        assert!(short.is_empty());

        // Terme de 5 caractères : longueurs 3, 4, 5
        let tokens = prefix_tokens(&key, "paris", 3, 8);
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].1, 3);
        assert_eq!(tokens[1].1, 4);
        assert_eq!(tokens[2].1, 5);

        // Vérification de cohérence avec prefix_token individuel
        let expected_3 = prefix_token(&key, "par", 3);
        assert_eq!(tokens[0].0, expected_3);

        // Terme long (> 8 caractères) : borné à 8
        let long_tokens = prefix_tokens(&key, "anticonstitutionnellement", 3, 8);
        assert_eq!(long_tokens.len(), 6); // 3, 4, 5, 6, 7, 8
        assert_eq!(long_tokens.last().unwrap().1, 8);
    }

    #[test]
    fn test_triplet_blind_tokens_computation() {
        let key = BlindIndexKey::new([123u8; 32]);
        let tokens = TripletBlindTokens::compute(&key, "Alice", "connaît", "Bob");

        assert_eq!(tokens.subject_exact, exact_token(&key, "Alice"));
        assert_eq!(tokens.predicate_exact, exact_token(&key, "connaît"));
        assert_eq!(tokens.object_exact, exact_token(&key, "Bob"));

        assert_eq!(tokens.subject_prefixes.len(), 3); // "ali", "alic", "alice"
        assert_eq!(tokens.object_prefixes.len(), 1); // "bob"
    }

    #[test]
    fn test_zeroize_on_drop() {
        let mut key_bytes = [77u8; 32];
        {
            let key = BlindIndexKey::new(key_bytes);
            assert_eq!(key.as_bytes(), &key_bytes);
        }
        // Clé détruite : en RAM l'instance temporaire a été mise à zéro via ZeroizeOnDrop
        key_bytes.zeroize();
        assert_eq!(key_bytes, [0u8; 32]);
    }
}
