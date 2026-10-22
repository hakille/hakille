//! File d'envoi hors-ligne (spec §8C, §19, §27) : « Je l'enverrai à ta
//! reconnexion », file visible dans le Journal.
//!
//! Reprise sûre : chaque entrée porte une clé d'idempotence (rejouable sans
//! doublon). Un envoi différé de plus de 15 minutes est REVALIDÉ avant de
//! partir (jamais d'envoi surprise). La file est consultable à tout moment.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Délai au-delà duquel un envoi différé doit être revalidé (minutes).
pub const REVALIDATE_AFTER_MINUTES: i64 = 15;

/// Une action mise en file : verbe, objet, paramètres, idempotence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutboxEntry {
    /// Clé d'idempotence : même clé rejouée = un seul envoi.
    pub dedup_key: String,
    pub verb: String,
    pub object: String,
    pub queued_at: DateTime<Utc>,
    /// Revalidée par l'utilisateur après 15 min d'attente ?
    pub revalidated: bool,
    /// Nombre de tentatives d'envoi déjà effectuées.
    pub attempts: u32,
}

impl OutboxEntry {
    pub fn new(verb: &str, object: &str, dedup_key: &str) -> Self {
        Self {
            dedup_key: dedup_key.to_string(),
            verb: verb.to_string(),
            object: object.to_string(),
            queued_at: Utc::now(),
            revalidated: false,
            attempts: 0,
        }
    }

    /// Minutes écoulées depuis la mise en file.
    pub fn waiting_minutes(&self, now: DateTime<Utc>) -> i64 {
        (now - self.queued_at).num_minutes()
    }

    /// L'entrée peut-elle partir maintenant ? Non si elle attend depuis plus
    /// de 15 minutes sans revalidation explicite.
    pub fn sendable(&self, now: DateTime<Utc>) -> bool {
        if self.waiting_minutes(now) > REVALIDATE_AFTER_MINUTES {
            self.revalidated
        } else {
            true
        }
    }
}

/// File d'envoi : ordre FIFO, dédupliquée par clé, visible dans le Journal.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Outbox {
    entries: Vec<OutboxEntry>,
}

impl Outbox {
    pub fn new() -> Self {
        Self::default()
    }

    /// Met en file ; une clé déjà présente est ignorée (rejeu sans doublon).
    /// Retourne true si nouvellement ajoutée.
    pub fn enqueue(&mut self, entry: OutboxEntry) -> bool {
        if self.entries.iter().any(|e| e.dedup_key == entry.dedup_key) {
            return false;
        }
        self.entries.push(entry);
        true
    }

    /// Prend la plus ancienne entrée envoyable (la marque d'une tentative).
    pub fn take_sendable(&mut self, now: DateTime<Utc>) -> Option<OutboxEntry> {
        let pos = self.entries.iter().position(|e| e.sendable(now))?;
        let mut entry = self.entries.remove(pos);
        entry.attempts += 1;
        Some(entry)
    }

    /// Revalide une entrée en attente (geste utilisateur explicite).
    pub fn revalidate(&mut self, dedup_key: &str) -> bool {
        match self.entries.iter_mut().find(|e| e.dedup_key == dedup_key) {
            Some(e) => {
                e.revalidated = true;
                true
            }
            None => false,
        }
    }

    /// Retire une entrée par clé d'idempotence.
    ///
    /// Utilisé pour **annuler un effet** dont la validation a échoué
    /// (ex. : audit indisponible après la mise en file d'une carte
    /// approuvée) : rien ne doit rester durable si la décision n'a pas
    /// été validée.
    pub fn remove(&mut self, dedup_key: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.dedup_key != dedup_key);
        self.entries.len() != before
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn list(&self) -> &[OutboxEntry] {
        &self.entries
    }

    /// File en attente au boot : une ligne lisible par entrée
    /// (`verbe — objet [clé, en attente N min, revalidée?]`).
    /// Appelée par le boot pour afficher la file (Journal visible).
    /// File vide ⇒ une seule ligne explicite (jamais de silence).
    pub fn list_pending(&self) -> Vec<String> {
        if self.entries.is_empty() {
            return vec!["Outbox: file vide — rien en attente.".to_string()];
        }
        let now = chrono::Utc::now();
        self.entries
            .iter()
            .map(|e| {
                format!(
                    "Outbox: {} — {} [clé {}, en attente {} min, {}]",
                    e.verb,
                    e.object,
                    e.dedup_key,
                    e.waiting_minutes(now),
                    if e.sendable(now) {
                        "envoyable"
                    } else if e.revalidated {
                        "revalidée"
                    } else {
                        "revalidation requise (>15 min)"
                    }
                )
            })
            .collect()
    }

    /// Persiste la file en JSON (reprise après redémarrage).
    pub fn save_to_file(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(&self.entries)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        std::fs::write(path.as_ref(), bytes)
    }

    /// Charge la file depuis un JSON écrit par [`Self::save_to_file`].
    /// Fichier absent ⇒ file vide (pas d'erreur). Les doublons éventuels
    /// du fichier sont dédupliqués à l'import (même sémantique que `enqueue`).
    pub fn load_from_file(path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        match std::fs::read(path.as_ref()) {
            Ok(bytes) => {
                let entries: Vec<OutboxEntry> = serde_json::from_slice(&bytes).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
                })?;
                let mut outbox = Outbox::new();
                for e in entries {
                    outbox.enqueue(e);
                }
                Ok(outbox)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Outbox::new()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_outoutboxdedup_replay_safe() {
        let mut outbox = Outbox::new();
        assert!(outbox.enqueue(OutboxEntry::new("Envoyer", "mail à Awa", "mail-1")));
        // Rejeu : même clé → ignoré, pas de doublon.
        assert!(!outbox.enqueue(OutboxEntry::new("Envoyer", "mail à Awa", "mail-1")));
        assert_eq!(outbox.len(), 1);
    }

    #[test]
    fn test_outoutbox15min_revalidation() {
        let mut outbox = Outbox::new();
        let mut old = OutboxEntry::new("Envoyer", "mail à Awa", "mail-2");
        old.queued_at = Utc::now() - Duration::minutes(16);
        outbox.enqueue(old);
        let now = Utc::now();
        // 16 min sans revalidation → bloqué.
        assert!(outbox.take_sendable(now).is_none());
        assert_eq!(outbox.len(), 1);
        // Geste explicite → part.
        assert!(outbox.revalidate("mail-2"));
        let sent = outbox.take_sendable(now).expect("revalidé : doit partir");
        assert_eq!(sent.attempts, 1);
        assert!(outbox.is_empty());
        assert_eq!(REVALIDATE_AFTER_MINUTES, 15);
    }

    #[test]
    fn test_outoutboxfifo_fresh_first() {
        let mut outbox = Outbox::new();
        outbox.enqueue(OutboxEntry::new("A", "un", "k1"));
        outbox.enqueue(OutboxEntry::new("B", "deux", "k2"));
        let now = Utc::now();
        assert_eq!(outbox.take_sendable(now).unwrap().dedup_key, "k1");
        assert_eq!(outbox.take_sendable(now).unwrap().dedup_key, "k2");
    }

    #[test]
    fn test_outbox_perf_enqueue_take_5k() {
        let mut outbox = Outbox::new();
        let start = std::time::Instant::now();
        for i in 0..5000 {
            assert!(outbox.enqueue(OutboxEntry::new("A", "objet", &format!("perf-{i}"))));
        }
        assert_eq!(outbox.len(), 5000);
        let now = Utc::now();
        for _ in 0..5000 {
            outbox.take_sendable(now).expect("file non vide");
        }
        assert!(outbox.is_empty());
        assert!(
            start.elapsed().as_secs() < 2,
            "budget perf dépassé : {:?}",
            start.elapsed()
        );
    }

    #[test]
    fn test_outbox_save_load_json_roundtrip() {
        let mut outbox = Outbox::new();
        outbox.enqueue(OutboxEntry::new("Envoyer", "mail à Awa", "mail-1"));
        outbox.enqueue(OutboxEntry::new("Envoyer", "sms à Moussa", "sms-1"));

        let dir = std::env::temp_dir().join(format!("hakille-outbox-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("outbox.json");
        outbox.save_to_file(&path).unwrap();

        let reloaded = Outbox::load_from_file(&path).unwrap();
        assert_eq!(reloaded.len(), 2);
        assert_eq!(reloaded.list()[0].dedup_key, "mail-1");

        // Fichier absent ⇒ file vide, pas d'erreur.
        let missing = Outbox::load_from_file(dir.join("absent.json")).unwrap();
        assert!(missing.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }
}
