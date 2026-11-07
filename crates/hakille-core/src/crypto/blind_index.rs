use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::HashMap;

type HmacSha256 = Hmac<Sha256>;

/// Normalize a term for blind indexing:
/// 1. Trim leading and trailing whitespace
/// 2. Convert to Unicode lowercase
/// 3. Collapse multiple contiguous whitespace characters to a single space
pub fn normalize_term(term: &str) -> String {
    let mut normalized = String::with_capacity(term.len());
    let mut last_was_space = false;

    for ch in term.trim().chars().flat_map(|c| c.to_lowercase()) {
        if ch.is_whitespace() {
            if !last_was_space {
                normalized.push(' ');
                last_was_space = true;
            }
        } else {
            normalized.push(ch);
            last_was_space = false;
        }
    }
    normalized
}

/// Compute an exact blind index token for a plaintext term using HMAC-SHA256
pub fn compute_blind_index(term: &str, blind_key: &[u8; 32]) -> [u8; 32] {
    let normalized = normalize_term(term);
    let mut mac = HmacSha256::new_from_slice(blind_key).expect("HMAC accepts 32-byte keys");
    mac.update(b"hakille:blind:exact:v1:");
    mac.update(normalized.as_bytes());
    mac.finalize().into_bytes().into()
}

/// Compute a prefix blind index token safely respecting Unicode character boundaries
pub fn compute_blind_prefix_index(term: &str, prefix_len: usize, blind_key: &[u8; 32]) -> [u8; 32] {
    let normalized = normalize_term(term);
    let prefix: String = normalized.chars().take(prefix_len).collect();

    let mut mac = HmacSha256::new_from_slice(blind_key).expect("HMAC accepts 32-byte keys");
    mac.update(b"hakille:blind:prefix:v1:");
    mac.update(&prefix_len.to_le_bytes());
    mac.update(b":");
    mac.update(prefix.as_bytes());
    mac.finalize().into_bytes().into()
}

/// In-memory searchable Zero-Knowledge Blind Index Store
#[derive(Debug, Default, Clone)]
pub struct BlindIndexStore {
    exact_index: HashMap<[u8; 32], Vec<String>>,
    prefix_index: HashMap<[u8; 32], Vec<String>>,
}

impl BlindIndexStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Index a record term under exact and prefix tokens
    pub fn insert(&mut self, term: &str, record_id: &str, blind_key: &[u8; 32]) {
        let exact = compute_blind_index(term, blind_key);
        self.exact_index
            .entry(exact)
            .or_default()
            .push(record_id.to_string());

        // Index prefixes from length 3 to 6
        let norm_char_count = normalize_term(term).chars().count();
        let max_prefix = norm_char_count.min(8);
        for p_len in 3..=max_prefix {
            let prefix_token = compute_blind_prefix_index(term, p_len, blind_key);
            self.prefix_index
                .entry(prefix_token)
                .or_default()
                .push(record_id.to_string());
        }
    }

    /// Search for records matching exact term
    pub fn search_exact(&self, term: &str, blind_key: &[u8; 32]) -> Vec<String> {
        let token = compute_blind_index(term, blind_key);
        self.exact_index.get(&token).cloned().unwrap_or_default()
    }

    /// Search for records matching prefix
    pub fn search_prefix(
        &self,
        prefix: &str,
        prefix_len: usize,
        blind_key: &[u8; 32],
    ) -> Vec<String> {
        let token = compute_blind_prefix_index(prefix, prefix_len, blind_key);
        self.prefix_index.get(&token).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blind_index_normalization_invariance() {
        let key = [0x99; 32];
        let token1 = compute_blind_index("  Alice   Smith  ", &key);
        let token2 = compute_blind_index("alice smith", &key);
        let token3 = compute_blind_index("ALICE   SMITH", &key);

        assert_eq!(token1, token2);
        assert_eq!(token2, token3);
    }

    #[test]
    fn test_blind_prefix_unicode_safety() {
        let key = [0x99; 32];
        // Multi-byte Unicode characters (French accents, emojis)
        let token = compute_blind_prefix_index("Équipe Sécurisée 🚀", 6, &key);
        assert_ne!(token, [0u8; 32]);
    }

    #[test]
    fn test_blind_index_store_search() {
        let mut store = BlindIndexStore::new();
        let key = [0x5a; 32];

        store.insert("Jean-Luc Picard", "rec-001", &key);
        store.insert("Jean Valjean", "rec-002", &key);
        store.insert("Beverly Crusher", "rec-003", &key);

        let exact_matches = store.search_exact("jean-luc picard", &key);
        assert_eq!(exact_matches, vec!["rec-001"]);

        let prefix_matches = store.search_prefix("jean", 4, &key);
        assert!(prefix_matches.contains(&"rec-001".to_string()));
        assert!(prefix_matches.contains(&"rec-002".to_string()));
        assert!(!prefix_matches.contains(&"rec-003".to_string()));
    }
}
