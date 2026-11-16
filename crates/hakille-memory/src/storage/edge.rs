//! Tier edge : persistance locale redb (`EdgeStore`).
//!
//! Honnêteté documentée (ne pas "améliorer" ces commentaires en
//! affirmations de performance non mesurées) :
//! - redb n'offre PAS de prefix-scan natif : `query_via_index` itère la
//!   table d'index et filtre par préfixe — complexité O(N) bornée au tier
//!   edge. Le routage SPO/POS/OSP choisit la table la plus sélective mais
//!   ne change pas la complexité asymptotique.
//! - CHIFRAGE : les CLÉS d'index SPO/POS/OSP sont stockées EN CLAIR
//!   (sujet / prédicat / objet lisibles sur disque) pour permettre le scan
//!   par préfixe. Seules les VALEURS (`ID_TABLE`) sont chiffrées quand une
//!   clé est fournie (ChaCha20-Poly1305 via `hakille-core`).
//!   TODO(chiffrement) : index chiffré interrogeable (ex. hachage
//!   déterministe du préfixe + nonce), rotation de clé, `crypto_erase`
//!   côté `heritage`.
//! - `batch_store` = UNE seule transaction redb ⇒ atomique (tout ou rien).
//! - `flush` = no-op documenté : chaque `commit()` redb est déjà durable ;
//!   `Database::compact` exigerait `&mut self`, incompatible avec le
//!   partage via `Arc<Database>`.
//! - Version de schéma `SCHEMA_VERSION` vérifiée à la construction
//!   (`try_new` strict ; `new` best-effort + log pour compatibilité).

use crate::storage::GraphStorage;
use crate::triplet::{TriplePattern, Triplet};
use async_trait::async_trait;
use hakille_core::{HakilleError, TripletId, decrypt_at_rest, encrypt_at_rest};
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use std::sync::Arc;

pub const SPO_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("spo");
pub const POS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("pos");
pub const OSP_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("osp");
pub const ID_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("id_to_triplet");
/// Table méta : stocke la version du schéma (clé `SCHEMA_VERSION_KEY`).
pub const META_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("hakille_meta");
/// Table embeddings : `id_triplet (bincode) → embedding (bincode Vec<f32>)`.
/// Dimension de production verrouillée : 384 (voir `vector::EMBEDDING_DIM).
/// Stockage en clair (vecteurs non sensibles par eux-mêmes) ; les triplets
/// restent chiffrés via `ID_TABLE` quand une clé est fournie.
pub const EMBEDDINGS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("id_to_embedding");
/// Table d'index aveugle exacte (ADR-012 §3) : `token_exact (32 octets HMAC) → Vec<TripletId> (bincode)`.
pub const BLIND_EXACT_TABLE: TableDefinition<&[u8; 32], &[u8]> =
    TableDefinition::new("blind_exact");
/// Table d'index aveugle par préfixe (ADR-012 §3) : `token_prefix (32 octets HMAC) → Vec<TripletId> (bincode)`.
pub const BLIND_PREFIX_TABLE: TableDefinition<&[u8; 32], &[u8]> =
    TableDefinition::new("blind_prefix");

/// Version courante du schéma des tables edge. Toute ouverture d'une base
/// portant une autre version échoue avec une erreur explicite (migration
/// manuelle requise — aucun upgrade silencieux).
pub const SCHEMA_VERSION: u32 = 1;
const SCHEMA_VERSION_KEY: &[u8] = b"hakille:schema_version";

pub struct EdgeStore {
    pub db: Arc<Database>,
    /// Clé de chiffrement au repos (32 octets). Si `None`, les valeurs sont
    /// stockées en clair (avec `tracing::warn!` à chaque `store`).
    encryption_key: Option<[u8; 32]>,
    /// Clé d'indexation aveugle dérivée pour les tables `BLIND_EXACT_TABLE` et
    /// `BLIND_PREFIX_TABLE` (ADR-012 §3), effacée de la mémoire à la destruction.
    blind_key: Option<crate::blind_index::BlindIndexKey>,
}

/// Vérifie (et initialise si absente) la version du schéma dans `META_TABLE`.
///
/// - Base fraîche : écrit `SCHEMA_VERSION`, Ok.
/// - Version égale : Ok.
/// - Version différente ou illisible : `Err` explicite, jamais de
///   migration silencieuse.
fn ensure_schema_version(db: &Database) -> hakille_core::Result<()> {
    let write_txn = db
        .begin_write()
        .map_err(|e| HakilleError::StorageError(format!("EdgeStore: begin_write (schéma): {e}")))?;
    {
        let mut meta = write_txn.open_table(META_TABLE).map_err(|e| {
            HakilleError::StorageError(format!("EdgeStore: open hakille_meta: {e}"))
        })?;
        // Copie possédée d'abord : le guard redb emprunte `meta`, on ne
        // peut pas `insert` tant qu'il est vivant (E0502).
        let current: Option<Vec<u8>> = meta
            .get(SCHEMA_VERSION_KEY)
            .map_err(|e| {
                HakilleError::StorageError(format!("EdgeStore: lecture version schéma: {e}"))
            })?
            .map(|guard| guard.value().to_vec());
        match current {
            None => {
                meta.insert(SCHEMA_VERSION_KEY, SCHEMA_VERSION.to_be_bytes().as_slice())
                    .map_err(|e| {
                        HakilleError::StorageError(format!(
                            "EdgeStore: écriture version schéma: {e}"
                        ))
                    })?;
            }
            Some(raw) => {
                if raw.len() != 4 {
                    return Err(HakilleError::StorageError(format!(
                        "EdgeStore: version de schéma illisible ({} octets, attendu 4) — base corrompue ?",
                        raw.len()
                    )));
                }
                let found = u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]);
                if found != SCHEMA_VERSION {
                    return Err(HakilleError::StorageError(format!(
                        "EdgeStore: version de schéma {found} inattendue (attendu {SCHEMA_VERSION}) — migration manuelle requise, refus d'ouvrir"
                    )));
                }
            }
        }
    }
    write_txn
        .commit()
        .map_err(|e| HakilleError::StorageError(format!("EdgeStore: commit (schéma): {e}")))?;
    Ok(())
}

impl EdgeStore {
    /// Construit sans vérifier strictement le schéma : en cas d'échec de
    /// vérification, une erreur est loggée (`tracing::error!`) et le store
    /// est quand même retourné. Conservé pour compatibilité — préférez
    /// `try_new()` pour tout nouveau code.
    pub fn new(db: Arc<Database>) -> Self {
        if let Err(e) = ensure_schema_version(&db) {
            tracing::error!(
                "EdgeStore::new: vérification du schéma impossible ({e}) — store retourné quand même, préférez try_new()"
            );
        }
        Self {
            db,
            encryption_key: None,
            blind_key: None,
        }
    }

    /// Construit en vérifiant strictement `SCHEMA_VERSION` : toute version
    /// absente est initialisée, toute version incompatible retourne `Err`.
    pub fn try_new(db: Arc<Database>) -> hakille_core::Result<Self> {
        ensure_schema_version(&db)?;
        Ok(Self {
            db,
            encryption_key: None,
            blind_key: None,
        })
    }

    /// Construit avec chiffrement au repos (`hakille-core` ChaCha20-Poly1305) et indexation aveugle (ADR-012).
    pub fn with_encryption_key(mut self, key: [u8; 32]) -> Self {
        self.encryption_key = Some(key);
        self.blind_key = Some(crate::blind_index::BlindIndexKey::new(key));
        self
    }

    pub fn set_encryption_key(&mut self, key: Option<[u8; 32]>) {
        self.encryption_key = key;
        self.blind_key = key.map(crate::blind_index::BlindIndexKey::new);
    }

    pub fn has_encryption(&self) -> bool {
        self.encryption_key.is_some()
    }

    /// Recherche aveugle exacte selon ADR-012 : recherche O(1) par HMAC-SHA256
    /// dans `BLIND_EXACT_TABLE` sans déchiffrer ni scanner l'intégralité du disque.
    pub async fn query_blind_exact(&self, term: &str) -> hakille_core::Result<Vec<Triplet>> {
        let bkey = match self.blind_key {
            Some(ref k) => k,
            None => {
                return Err(HakilleError::Internal(
                    "query_blind_exact requiert une clé de chiffrement".to_string(),
                ));
            }
        };
        let token = crate::blind_index::exact_token(bkey, term);
        let read_txn = self.db.begin_read().map_err(|e| {
            HakilleError::StorageError(format!("EdgeStore::query_blind_exact: begin_read: {e}"))
        })?;
        let table = match read_txn.open_table(BLIND_EXACT_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => {
                return Err(HakilleError::StorageError(format!(
                    "EdgeStore::query_blind_exact: {e}"
                )));
            }
        };
        let ids: Vec<TripletId> = match table.get(&token).map_err(|e| {
            HakilleError::StorageError(format!("EdgeStore::query_blind_exact: get: {e}"))
        })? {
            Some(entry) => bincode::deserialize(entry.value()).unwrap_or_default(),
            None => return Ok(Vec::new()),
        };
        let mut triplets = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(t) = self.load(&id).await? {
                triplets.push(t);
            }
        }
        Ok(triplets)
    }

    /// Recherche aveugle par préfixe selon ADR-012 : longueur bornée [3, 8],
    /// recherche O(1) par HMAC-SHA256 dans `BLIND_PREFIX_TABLE`.
    pub async fn query_blind_prefix(&self, prefix: &str) -> hakille_core::Result<Vec<Triplet>> {
        let bkey = match self.blind_key {
            Some(ref k) => k,
            None => {
                return Err(HakilleError::Internal(
                    "query_blind_prefix requiert une clé de chiffrement".to_string(),
                ));
            }
        };
        let norm = crate::blind_index::normalize(prefix);
        let char_len = norm.chars().count();
        if char_len < crate::blind_index::DEFAULT_MIN_PREFIX_LEN {
            return Err(HakilleError::Internal(format!(
                "query_blind_prefix: longueur {} < minimum {}",
                char_len,
                crate::blind_index::DEFAULT_MIN_PREFIX_LEN
            )));
        }
        let bounded_len = char_len.min(crate::blind_index::DEFAULT_MAX_PREFIX_LEN);
        let prefix_slice: String = norm.chars().take(bounded_len).collect();
        let token = crate::blind_index::prefix_token(bkey, &prefix_slice, bounded_len);
        let read_txn = self.db.begin_read().map_err(|e| {
            HakilleError::StorageError(format!("EdgeStore::query_blind_prefix: begin_read: {e}"))
        })?;
        let table = match read_txn.open_table(BLIND_PREFIX_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => {
                return Err(HakilleError::StorageError(format!(
                    "EdgeStore::query_blind_prefix: {e}"
                )));
            }
        };
        let ids: Vec<TripletId> = match table.get(&token).map_err(|e| {
            HakilleError::StorageError(format!("EdgeStore::query_blind_prefix: get: {e}"))
        })? {
            Some(entry) => bincode::deserialize(entry.value()).unwrap_or_default(),
            None => return Ok(Vec::new()),
        };
        let mut triplets = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(t) = self.load(&id).await? {
                triplets.push(t);
            }
        }
        Ok(triplets)
    }

    /// Persiste un embedding (`id → Vec<f32>`) dans `EMBEDDINGS_TABLE`.
    /// Dimension 384 verrouillée en production (voir `vector::EMBEDDING_DIM`) :
    /// toute autre dimension rend `Err` (jamais de troncature silencieuse).
    pub fn save_embedding(&self, id: &TripletId, embedding: &[f32]) -> hakille_core::Result<()> {
        if embedding.len() != crate::vector::EMBEDDING_DIM {
            return Err(HakilleError::Internal(format!(
                "EdgeStore::save_embedding: dimension {} refusée (verrouillée à {})",
                embedding.len(),
                crate::vector::EMBEDDING_DIM
            )));
        }
        let id_bytes = bincode::serialize(id).map_err(|e| HakilleError::Internal(e.to_string()))?;
        let vec_bytes =
            bincode::serialize(embedding).map_err(|e| HakilleError::Internal(e.to_string()))?;
        let txn = self
            .db
            .begin_write()
            .map_err(|e| HakilleError::Internal(e.to_string()))?;
        {
            let mut table = txn
                .open_table(EMBEDDINGS_TABLE)
                .map_err(|e| HakilleError::Internal(e.to_string()))?;
            table
                .insert(id_bytes.as_slice(), vec_bytes.as_slice())
                .map_err(|e| HakilleError::Internal(e.to_string()))?;
        }
        txn.commit()
            .map_err(|e| HakilleError::Internal(e.to_string()))?;
        Ok(())
    }

    /// Charge un embedding (`None` si absent ou base fraîche).
    pub fn load_embedding(&self, id: &TripletId) -> hakille_core::Result<Option<Vec<f32>>> {
        let id_bytes = bincode::serialize(id).map_err(|e| HakilleError::Internal(e.to_string()))?;
        let txn = self
            .db
            .begin_read()
            .map_err(|e| HakilleError::Internal(e.to_string()))?;
        let table = match txn.open_table(EMBEDDINGS_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(HakilleError::Internal(e.to_string())),
        };
        match table
            .get(id_bytes.as_slice())
            .map_err(|e| HakilleError::Internal(e.to_string()))?
        {
            Some(entry) => {
                let vec: Vec<f32> = bincode::deserialize(entry.value())
                    .map_err(|e| HakilleError::Internal(e.to_string()))?;
                Ok(Some(vec))
            }
            None => Ok(None),
        }
    }

    /// Tous les embeddings persistés (`(id, vec)`). Base fraîche ⇒ `Ok(vec![])`.
    pub fn all_embeddings(&self) -> hakille_core::Result<Vec<(TripletId, Vec<f32>)>> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| HakilleError::Internal(e.to_string()))?;
        let table = match txn.open_table(EMBEDDINGS_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(HakilleError::Internal(e.to_string())),
        };
        let mut out = Vec::new();
        for item in table
            .iter()
            .map_err(|e| HakilleError::Internal(e.to_string()))?
        {
            let (k, v) = item.map_err(|e| HakilleError::Internal(e.to_string()))?;
            let id: TripletId = bincode::deserialize(k.value())
                .map_err(|e| HakilleError::Internal(e.to_string()))?;
            let vec: Vec<f32> = bincode::deserialize(v.value())
                .map_err(|e| HakilleError::Internal(e.to_string()))?;
            out.push((id, vec));
        }
        Ok(out)
    }

    /// Supprime un triplet en indiquant s'il existait.
    ///
    /// - Absent (y compris base fraîche) : `Ok(false)` SANS créer les
    ///   tables — la vérification passe par `load()` en lecture seule avant
    ///   toute transaction d'écriture (ouvrir une table en écriture la
    ///   créerait, même pour un no-op).
    /// - Présent : suppression dans les 4 tables, `Ok(true)`.
    ///
    /// Nota : le diptyque `load` puis suppression n'est pas atomique
    /// (TOCTOU entre deux writers concurrents) — acceptable pour le tier
    /// edge mono-writer, documenté comme tel.
    pub async fn delete_returning(&self, id: &TripletId) -> hakille_core::Result<bool> {
        // Lecture d'abord : aucun write, donc aucune création de table.
        let existing = self.load(id).await?;
        let Some(t) = existing else {
            return Ok(false);
        };
        let id_bytes = bincode::serialize(id).map_err(|e| HakilleError::Internal(e.to_string()))?;
        let spo = spo_key(&t)?;
        let pos = pos_key(&t)?;
        let osp = osp_key(&t)?;
        let write_txn = self.db.begin_write().map_err(|e| {
            HakilleError::StorageError(format!("EdgeStore::delete: begin_write: {e}"))
        })?;
        {
            let mut id_table = write_txn.open_table(ID_TABLE).map_err(|e| {
                HakilleError::StorageError(format!("EdgeStore::delete: open id: {e}"))
            })?;
            id_table.remove(id_bytes.as_slice()).map_err(|e| {
                HakilleError::StorageError(format!("EdgeStore::delete: remove id: {e}"))
            })?;
            let mut spo_table = write_txn.open_table(SPO_TABLE).map_err(|e| {
                HakilleError::StorageError(format!("EdgeStore::delete: open spo: {e}"))
            })?;
            spo_table.remove(spo.as_slice()).map_err(|e| {
                HakilleError::StorageError(format!("EdgeStore::delete: remove spo: {e}"))
            })?;
            let mut pos_table = write_txn.open_table(POS_TABLE).map_err(|e| {
                HakilleError::StorageError(format!("EdgeStore::delete: open pos: {e}"))
            })?;
            pos_table.remove(pos.as_slice()).map_err(|e| {
                HakilleError::StorageError(format!("EdgeStore::delete: remove pos: {e}"))
            })?;
            let mut osp_table = write_txn.open_table(OSP_TABLE).map_err(|e| {
                HakilleError::StorageError(format!("EdgeStore::delete: open osp: {e}"))
            })?;
            osp_table.remove(osp.as_slice()).map_err(|e| {
                HakilleError::StorageError(format!("EdgeStore::delete: remove osp: {e}"))
            })?;

            if let Some(ref bkey) = self.blind_key
                && let Ok(mut exact_table) = write_txn.open_table(BLIND_EXACT_TABLE)
                && let Ok(mut prefix_table) = write_txn.open_table(BLIND_PREFIX_TABLE)
            {
                deindex_triplet_blind(&mut exact_table, &mut prefix_table, bkey, &t)?;
            }
        }
        write_txn
            .commit()
            .map_err(|e| HakilleError::StorageError(format!("EdgeStore::delete: commit: {e}")))?;
        Ok(true)
    }

    fn encode_value(&self, triplet: &Triplet) -> hakille_core::Result<Vec<u8>> {
        let plain =
            bincode::serialize(triplet).map_err(|e| HakilleError::Internal(e.to_string()))?;
        match self.encryption_key {
            Some(ref key) => {
                let blob = encrypt_at_rest(&plain, key)
                    .map_err(|e| HakilleError::Internal(e.to_string()))?;
                bincode::serialize(&blob).map_err(|e| HakilleError::Internal(e.to_string()))
            }
            None => Ok(plain),
        }
    }

    fn decode_value(&self, bytes: &[u8]) -> hakille_core::Result<Triplet> {
        match self.encryption_key {
            Some(ref key) => {
                let blob: hakille_core::EncryptedBlob = bincode::deserialize(bytes)
                    .map_err(|e| HakilleError::Internal(e.to_string()))?;
                let plain = decrypt_at_rest(&blob, key)
                    .map_err(|e| HakilleError::Internal(e.to_string()))?;
                bincode::deserialize(&plain).map_err(|e| HakilleError::Internal(e.to_string()))
            }
            None => bincode::deserialize(bytes).map_err(|e| HakilleError::Internal(e.to_string())),
        }
    }
}

fn node_value_text(v: &crate::triplet::NodeValue) -> Option<String> {
    match v {
        crate::triplet::NodeValue::Entity(id) => Some(id.0.to_string()),
        crate::triplet::NodeValue::Text(s) => Some(s.clone()),
        crate::triplet::NodeValue::Number(n) => Some(n.to_string()),
        crate::triplet::NodeValue::Boolean(b) => Some(b.to_string()),
        crate::triplet::NodeValue::Timestamp(ts) => Some(ts.0.to_rfc3339()),
        crate::triplet::NodeValue::Embedding(_) => None,
    }
}

fn add_triplet_id_to_blind_table(
    table: &mut redb::Table<&[u8; 32], &[u8]>,
    token: &[u8; 32],
    id: &TripletId,
) -> hakille_core::Result<()> {
    let mut ids: Vec<TripletId> = match table
        .get(token)
        .map_err(|e| HakilleError::StorageError(e.to_string()))?
    {
        Some(entry) => bincode::deserialize(entry.value()).unwrap_or_default(),
        None => Vec::new(),
    };
    if !ids.contains(id) {
        ids.push(id.clone());
        let encoded =
            bincode::serialize(&ids).map_err(|e| HakilleError::Internal(e.to_string()))?;
        table
            .insert(token, encoded.as_slice())
            .map_err(|e| HakilleError::StorageError(e.to_string()))?;
    }
    Ok(())
}

fn remove_triplet_id_from_blind_table(
    table: &mut redb::Table<&[u8; 32], &[u8]>,
    token: &[u8; 32],
    id: &TripletId,
) -> hakille_core::Result<()> {
    let existing: Option<Vec<TripletId>> = match table
        .get(token)
        .map_err(|e| HakilleError::StorageError(e.to_string()))?
    {
        Some(entry) => bincode::deserialize(entry.value()).ok(),
        None => None,
    };
    if let Some(mut ids) = existing
        && let Some(pos) = ids.iter().position(|x| x == id)
    {
        ids.swap_remove(pos);
        if ids.is_empty() {
            let _ = table.remove(token);
        } else {
            let encoded =
                bincode::serialize(&ids).map_err(|e| HakilleError::Internal(e.to_string()))?;
            table
                .insert(token, encoded.as_slice())
                .map_err(|e| HakilleError::StorageError(e.to_string()))?;
        }
    }
    Ok(())
}

fn index_text_blind(
    exact_table: &mut redb::Table<&[u8; 32], &[u8]>,
    prefix_table: &mut redb::Table<&[u8; 32], &[u8]>,
    key: &crate::blind_index::BlindIndexKey,
    text: &str,
    id: &TripletId,
) -> hakille_core::Result<()> {
    let exact = crate::blind_index::exact_token(key, text);
    add_triplet_id_to_blind_table(exact_table, &exact, id)?;

    let norm = crate::blind_index::normalize(text);
    if norm.contains(' ') {
        for word in norm.split_whitespace() {
            let w_exact = crate::blind_index::exact_token(key, word);
            add_triplet_id_to_blind_table(exact_table, &w_exact, id)?;
        }
    }

    for (p_tok, _) in crate::blind_index::prefix_tokens(
        key,
        text,
        crate::blind_index::DEFAULT_MIN_PREFIX_LEN,
        crate::blind_index::DEFAULT_MAX_PREFIX_LEN,
    ) {
        add_triplet_id_to_blind_table(prefix_table, &p_tok, id)?;
    }

    Ok(())
}

fn deindex_text_blind(
    exact_table: &mut redb::Table<&[u8; 32], &[u8]>,
    prefix_table: &mut redb::Table<&[u8; 32], &[u8]>,
    key: &crate::blind_index::BlindIndexKey,
    text: &str,
    id: &TripletId,
) -> hakille_core::Result<()> {
    let exact = crate::blind_index::exact_token(key, text);
    remove_triplet_id_from_blind_table(exact_table, &exact, id)?;

    let norm = crate::blind_index::normalize(text);
    if norm.contains(' ') {
        for word in norm.split_whitespace() {
            let w_exact = crate::blind_index::exact_token(key, word);
            remove_triplet_id_from_blind_table(exact_table, &w_exact, id)?;
        }
    }

    for (p_tok, _) in crate::blind_index::prefix_tokens(
        key,
        text,
        crate::blind_index::DEFAULT_MIN_PREFIX_LEN,
        crate::blind_index::DEFAULT_MAX_PREFIX_LEN,
    ) {
        remove_triplet_id_from_blind_table(prefix_table, &p_tok, id)?;
    }

    Ok(())
}

fn index_triplet_blind(
    exact_table: &mut redb::Table<&[u8; 32], &[u8]>,
    prefix_table: &mut redb::Table<&[u8; 32], &[u8]>,
    key: &crate::blind_index::BlindIndexKey,
    triplet: &Triplet,
) -> hakille_core::Result<()> {
    let subj_str = triplet.subject.0.to_string();
    index_text_blind(exact_table, prefix_table, key, &subj_str, &triplet.id)?;

    let pred_str = triplet.predicate.0.to_string();
    index_text_blind(exact_table, prefix_table, key, &pred_str, &triplet.id)?;

    if let Some(obj_text) = node_value_text(&triplet.object) {
        index_text_blind(exact_table, prefix_table, key, &obj_text, &triplet.id)?;
    }

    Ok(())
}

fn deindex_triplet_blind(
    exact_table: &mut redb::Table<&[u8; 32], &[u8]>,
    prefix_table: &mut redb::Table<&[u8; 32], &[u8]>,
    key: &crate::blind_index::BlindIndexKey,
    triplet: &Triplet,
) -> hakille_core::Result<()> {
    let subj_str = triplet.subject.0.to_string();
    deindex_text_blind(exact_table, prefix_table, key, &subj_str, &triplet.id)?;

    let pred_str = triplet.predicate.0.to_string();
    deindex_text_blind(exact_table, prefix_table, key, &pred_str, &triplet.id)?;

    if let Some(obj_text) = node_value_text(&triplet.object) {
        deindex_text_blind(exact_table, prefix_table, key, &obj_text, &triplet.id)?;
    }

    Ok(())
}

/// Encodage documenté des composants pour les clés d'index concaténées
/// (mission 8) : UUID = 16 octets bruts ; objet = `bincode(NodeValue)`
/// (déterministe pour une même valeur) ; ID triplet suffixé pour garantir
/// l'unicité des clés même à S/P/O identiques.
///
/// CHIFRAGE : ces clés sont en clair sur disque (voir doc module).
/// Les erreurs `bincode` sont propagées via `map_err` — jamais de clé vide
/// silencieuse (`unwrap_or_default` interdit : une clé vide corromprait les
/// 3 index en collisions muettes).
fn subject_bytes(t: &Triplet) -> Vec<u8> {
    t.subject.0.as_bytes().to_vec()
}

fn predicate_bytes(t: &Triplet) -> Vec<u8> {
    t.predicate.0.as_bytes().to_vec()
}

fn object_bytes_for_triplet(t: &Triplet) -> hakille_core::Result<Vec<u8>> {
    bincode::serialize(&t.object).map_err(|e| HakilleError::Internal(e.to_string()))
}

fn object_bytes_for_value(v: &crate::triplet::NodeValue) -> hakille_core::Result<Vec<u8>> {
    bincode::serialize(v).map_err(|e| HakilleError::Internal(e.to_string()))
}

fn id_bytes_for_triplet(t: &Triplet) -> hakille_core::Result<Vec<u8>> {
    bincode::serialize(&t.id).map_err(|e| HakilleError::Internal(e.to_string()))
}

/// Clé SPO = sujet ‖ prédicat ‖ objet ‖ id (propriétés d'une entité).
fn spo_key(t: &Triplet) -> hakille_core::Result<Vec<u8>> {
    let mut k = Vec::new();
    k.extend_from_slice(&subject_bytes(t));
    k.extend_from_slice(&predicate_bytes(t));
    k.extend_from_slice(&object_bytes_for_triplet(t)?);
    k.extend_from_slice(&id_bytes_for_triplet(t)?);
    Ok(k)
}

/// Clé POS = prédicat ‖ objet ‖ sujet ‖ id (sujets par prédicat+valeur).
fn pos_key(t: &Triplet) -> hakille_core::Result<Vec<u8>> {
    let mut k = Vec::new();
    k.extend_from_slice(&predicate_bytes(t));
    k.extend_from_slice(&object_bytes_for_triplet(t)?);
    k.extend_from_slice(&subject_bytes(t));
    k.extend_from_slice(&id_bytes_for_triplet(t)?);
    Ok(k)
}

/// Clé OSP = objet ‖ sujet ‖ prédicat ‖ id (recherche inversée objet→sujet).
fn osp_key(t: &Triplet) -> hakille_core::Result<Vec<u8>> {
    let mut k = Vec::new();
    k.extend_from_slice(&object_bytes_for_triplet(t)?);
    k.extend_from_slice(&subject_bytes(t));
    k.extend_from_slice(&predicate_bytes(t));
    k.extend_from_slice(&id_bytes_for_triplet(t)?);
    Ok(k)
}

fn pattern_matches(pattern: &TriplePattern, t: &Triplet) -> bool {
    let s_match = pattern.subject.as_ref().is_none_or(|s| s == &t.subject);
    let p_match = pattern.predicate.as_ref().is_none_or(|p| p == &t.predicate);
    let o_match = pattern.object.as_ref().is_none_or(|o| o == &t.object);
    s_match && p_match && o_match
}

#[async_trait]
impl GraphStorage for EdgeStore {
    async fn store(&self, triplet: &Triplet) -> hakille_core::Result<()> {
        if self.encryption_key.is_none() {
            tracing::warn!(
                "EdgeStore::store en clair (aucune clé de chiffrement fournie) — voir TODO(chiffrement)"
            );
        }
        let id_bytes =
            bincode::serialize(&triplet.id).map_err(|e| HakilleError::Internal(e.to_string()))?;
        let triplet_bytes = self.encode_value(triplet)?;
        // Les 3 index sont écrits dans la même transaction ACID que la
        // valeur : fini les tables SPO/POS/OSP mortes (mission 8).
        let spo = spo_key(triplet)?;
        let pos = pos_key(triplet)?;
        let osp = osp_key(triplet)?;

        let write_txn = self.db.begin_write().map_err(|e| {
            HakilleError::StorageError(format!("EdgeStore::store: begin_write: {e}"))
        })?;
        {
            let mut id_table = write_txn.open_table(ID_TABLE).map_err(|e| {
                HakilleError::StorageError(format!("EdgeStore::store: open id: {e}"))
            })?;
            id_table
                .insert(id_bytes.as_slice(), triplet_bytes.as_slice())
                .map_err(|e| {
                    HakilleError::StorageError(format!("EdgeStore::store: insert id: {e}"))
                })?;
            let mut spo_table = write_txn.open_table(SPO_TABLE).map_err(|e| {
                HakilleError::StorageError(format!("EdgeStore::store: open spo: {e}"))
            })?;
            spo_table
                .insert(spo.as_slice(), id_bytes.as_slice())
                .map_err(|e| {
                    HakilleError::StorageError(format!("EdgeStore::store: insert spo: {e}"))
                })?;
            let mut pos_table = write_txn.open_table(POS_TABLE).map_err(|e| {
                HakilleError::StorageError(format!("EdgeStore::store: open pos: {e}"))
            })?;
            pos_table
                .insert(pos.as_slice(), id_bytes.as_slice())
                .map_err(|e| {
                    HakilleError::StorageError(format!("EdgeStore::store: insert pos: {e}"))
                })?;
            let mut osp_table = write_txn.open_table(OSP_TABLE).map_err(|e| {
                HakilleError::StorageError(format!("EdgeStore::store: open osp: {e}"))
            })?;
            osp_table
                .insert(osp.as_slice(), id_bytes.as_slice())
                .map_err(|e| {
                    HakilleError::StorageError(format!("EdgeStore::store: insert osp: {e}"))
                })?;

            if let Some(ref bkey) = self.blind_key {
                let mut exact_table = write_txn.open_table(BLIND_EXACT_TABLE).map_err(|e| {
                    HakilleError::StorageError(format!("EdgeStore::store: open blind_exact: {e}"))
                })?;
                let mut prefix_table = write_txn.open_table(BLIND_PREFIX_TABLE).map_err(|e| {
                    HakilleError::StorageError(format!("EdgeStore::store: open blind_prefix: {e}"))
                })?;
                index_triplet_blind(&mut exact_table, &mut prefix_table, bkey, triplet)?;
            }
        }
        write_txn
            .commit()
            .map_err(|e| HakilleError::StorageError(format!("EdgeStore::store: commit: {e}")))?;
        Ok(())
    }

    /// Stocke un lot en UNE seule transaction redb ⇒ atomique : soit tous
    /// les triplets sont persistés (avec leurs 3 index), soit aucun
    /// (échec avant ou pendant le `commit`, y compris pré-encodage —
    /// toute erreur de sérialisation avorte AVANT l'ouverture en écriture).
    async fn batch_store(&self, triplets: Vec<Triplet>) -> hakille_core::Result<()> {
        if triplets.is_empty() {
            return Ok(());
        }
        if self.encryption_key.is_none() {
            tracing::warn!(
                "EdgeStore::batch_store en clair (aucune clé de chiffrement fournie) — voir TODO(chiffrement)"
            );
        }
        // Pré-encodage TOTAL avant toute écriture : une erreur ici ne
        // laisse aucun état partiel (atomicité dès la préparation).
        let mut rows = Vec::with_capacity(triplets.len());
        for t in &triplets {
            let id_bytes =
                bincode::serialize(&t.id).map_err(|e| HakilleError::Internal(e.to_string()))?;
            let triplet_bytes = self.encode_value(t)?;
            let spo = spo_key(t)?;
            let pos = pos_key(t)?;
            let osp = osp_key(t)?;
            rows.push((id_bytes, triplet_bytes, spo, pos, osp));
        }

        let write_txn = self.db.begin_write().map_err(|e| {
            HakilleError::StorageError(format!("EdgeStore::batch_store: begin_write: {e}"))
        })?;
        {
            let mut id_table = write_txn.open_table(ID_TABLE).map_err(|e| {
                HakilleError::StorageError(format!("EdgeStore::batch_store: open id: {e}"))
            })?;
            let mut spo_table = write_txn.open_table(SPO_TABLE).map_err(|e| {
                HakilleError::StorageError(format!("EdgeStore::batch_store: open spo: {e}"))
            })?;
            let mut pos_table = write_txn.open_table(POS_TABLE).map_err(|e| {
                HakilleError::StorageError(format!("EdgeStore::batch_store: open pos: {e}"))
            })?;
            let mut osp_table = write_txn.open_table(OSP_TABLE).map_err(|e| {
                HakilleError::StorageError(format!("EdgeStore::batch_store: open osp: {e}"))
            })?;
            for (id_bytes, triplet_bytes, spo, pos, osp) in &rows {
                id_table
                    .insert(id_bytes.as_slice(), triplet_bytes.as_slice())
                    .map_err(|e| {
                        HakilleError::StorageError(format!(
                            "EdgeStore::batch_store: insert id: {e}"
                        ))
                    })?;
                spo_table
                    .insert(spo.as_slice(), id_bytes.as_slice())
                    .map_err(|e| {
                        HakilleError::StorageError(format!(
                            "EdgeStore::batch_store: insert spo: {e}"
                        ))
                    })?;
                pos_table
                    .insert(pos.as_slice(), id_bytes.as_slice())
                    .map_err(|e| {
                        HakilleError::StorageError(format!(
                            "EdgeStore::batch_store: insert pos: {e}"
                        ))
                    })?;
                osp_table
                    .insert(osp.as_slice(), id_bytes.as_slice())
                    .map_err(|e| {
                        HakilleError::StorageError(format!(
                            "EdgeStore::batch_store: insert osp: {e}"
                        ))
                    })?;
            }

            if let Some(ref bkey) = self.blind_key {
                let mut exact_table = write_txn.open_table(BLIND_EXACT_TABLE).map_err(|e| {
                    HakilleError::StorageError(format!(
                        "EdgeStore::batch_store: open blind_exact: {e}"
                    ))
                })?;
                let mut prefix_table = write_txn.open_table(BLIND_PREFIX_TABLE).map_err(|e| {
                    HakilleError::StorageError(format!(
                        "EdgeStore::batch_store: open blind_prefix: {e}"
                    ))
                })?;
                for t in &triplets {
                    index_triplet_blind(&mut exact_table, &mut prefix_table, bkey, t)?;
                }
            }
        }
        write_txn.commit().map_err(|e| {
            HakilleError::StorageError(format!("EdgeStore::batch_store: commit: {e}"))
        })?;
        Ok(())
    }

    /// Compte O(1) via `ReadableTable::len()` ; base fraîche ⇒ `Ok(0)`
    /// (table absente ≠ erreur).
    async fn count(&self) -> hakille_core::Result<usize> {
        let read_txn = self.db.begin_read().map_err(|e| {
            HakilleError::StorageError(format!("EdgeStore::count: begin_read: {e}"))
        })?;
        let id_table = match read_txn.open_table(ID_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(0),
            Err(e) => {
                return Err(HakilleError::StorageError(format!(
                    "EdgeStore::count: open id_to_triplet: {e}"
                )));
            }
        };
        let n = id_table
            .len()
            .map_err(|e| HakilleError::StorageError(format!("EdgeStore::count: len: {e}")))?;
        usize::try_from(n).map_err(|e| {
            HakilleError::Internal(format!("EdgeStore::count: débordement usize: {e}"))
        })
    }

    /// No-op DOCUMENTÉ : chaque `commit()` redb est déjà durable (journal +
    /// fsync au commit) ; `Database::compact` exigerait `&mut self`,
    /// incompatible avec le partage via `Arc<Database>`. Rien à flusher.
    async fn flush(&self) -> hakille_core::Result<()> {
        Ok(())
    }

    async fn load(&self, id: &TripletId) -> hakille_core::Result<Option<Triplet>> {
        let id_bytes = bincode::serialize(id).map_err(|e| HakilleError::Internal(e.to_string()))?;
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| HakilleError::StorageError(format!("EdgeStore::load: begin_read: {e}")))?;
        let id_table = match read_txn.open_table(ID_TABLE) {
            Ok(t) => t,
            // Base fraîche : table jamais créée ⇒ rien à charger (pas d'erreur).
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => {
                return Err(HakilleError::StorageError(format!(
                    "EdgeStore::load: open id_to_triplet: {e}"
                )));
            }
        };
        if let Some(entry) = id_table
            .get(id_bytes.as_slice())
            .map_err(|e| HakilleError::StorageError(format!("EdgeStore::load: get: {e}")))?
        {
            let triplet = self.decode_value(entry.value())?;
            Ok(Some(triplet))
        } else {
            Ok(None)
        }
    }

    async fn query(&self, pattern: &TriplePattern) -> hakille_core::Result<Vec<Triplet>> {
        // Routage honnête vers l'index le plus sélectif (mission 8) :
        // - sujet (+prédicat) ⇒ SPO par préfixe s[+p] ;
        // - prédicat seul ⇒ POS par préfixe p ;
        // - objet seul ⇒ OSP par préfixe o.
        // Nota : redb n'offre pas de prefix-scan natif ; on itère la table
        // index et on filtre par préfixe — O(N) borné au tier edge, documenté
        // comme tel.
        match self.query_via_index(pattern).await {
            // Chemin indexé nominal.
            Some(Ok(hits)) => Ok(hits),
            // Erreur redb / corruption / déchiffrement : PROPAGÉE telle
            // quelle. (Avant : avalée silencieusement puis masquée par un
            // fallback scan aveugle qui pouvait retourner un résultat
            // partiel ou vide sans signaler la panne.)
            Some(Err(e)) => Err(e),
            // Pas d'index sélectif (pattern vide) ou base fraîche (tables
            // d'index absentes) : fallback scan+filtre O(N), légitime et
            // loggé via `tracing::warn!`.
            None => {
                tracing::warn!(
                    "EdgeStore::query: aucun index sélectif — fallback scan+filtre O(N) sur id_to_triplet"
                );
                let triplets = self.all_triplets().await?;
                Ok(triplets
                    .into_iter()
                    .filter(|t| pattern_matches(pattern, t))
                    .collect())
            }
        }
    }

    /// Conservée pour compatibilité (`GraphStorage`) : délègue à
    /// `delete_returning()` en ignorant le booléen. Ne crée aucune table
    /// sur no-op (voir `delete_returning`).
    async fn delete(&self, id: &TripletId) -> hakille_core::Result<()> {
        self.delete_returning(id).await.map(|_| ())
    }

    async fn all_triplets(&self) -> hakille_core::Result<Vec<Triplet>> {
        let read_txn = self.db.begin_read().map_err(|e| {
            HakilleError::StorageError(format!("EdgeStore::all_triplets: begin_read: {e}"))
        })?;
        let id_table = match read_txn.open_table(ID_TABLE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => {
                return Err(HakilleError::StorageError(format!(
                    "EdgeStore::all_triplets: open id_to_triplet: {e}"
                )));
            }
        };
        let mut results = Vec::new();
        for item in id_table.iter().map_err(|e| {
            HakilleError::StorageError(format!("EdgeStore::all_triplets: iter: {e}"))
        })? {
            let (_k, v) = item.map_err(|e| {
                HakilleError::StorageError(format!("EdgeStore::all_triplets: lecture entrée: {e}"))
            })?;
            let triplet = self.decode_value(v.value())?;
            results.push(triplet);
        }
        Ok(results)
    }
}

impl EdgeStore {
    /// Chemin indexé.
    /// - `None` : aucun index sélectif (pattern vide) OU base fraîche
    ///   (table d'index jamais créée) → l'appelant retombe sur le scan
    ///   avec `warn!` (fallback légitime).
    /// - `Some(Ok)` : hits via l'index.
    /// - `Some(Err)` : erreur redb / corruption / déchiffrement réelle →
    ///   PROPAGÉE, jamais convertie en fallback silencieux.
    ///
    /// Chaque `.ok()?` historique (qui confondait "pas d'index" et "erreur
    /// redb") est remplacé par une gestion explicite `Result` → `Option`
    /// avec log quand le fallback est légitime.
    async fn query_via_index(
        &self,
        pattern: &TriplePattern,
    ) -> Option<hakille_core::Result<Vec<Triplet>>> {
        enum Index {
            Spo,
            Pos,
            Osp,
        }
        // 1. Sélection de l'index + préfixe.
        let (index, index_name, prefix): (Index, &'static str, Vec<u8>) =
            if let Some(ref s) = pattern.subject {
                let mut p = s.0.as_bytes().to_vec();
                if let Some(ref pred) = pattern.predicate {
                    p.extend_from_slice(pred.0.as_bytes());
                }
                (Index::Spo, "spo", p)
            } else if let Some(ref pred) = pattern.predicate {
                // POS sans sujet : préfixe prédicat seul.
                (Index::Pos, "pos", pred.0.as_bytes().to_vec())
            } else if let Some(ref obj) = pattern.object {
                // OSP sans sujet ni prédicat : préfixe objet sérialisé.
                // Échec de sérialisation ⇒ erreur réelle (un objet non
                // sérialisable n'aurait jamais pu être stocké) : propagée,
                // jamais avalée.
                match object_bytes_for_value(obj) {
                    Ok(b) => (Index::Osp, "osp", b),
                    Err(e) => return Some(Err(e)),
                }
            } else {
                // Pattern vide : aucun index sélectif (fallback légitime).
                return None;
            };

        // 2. Transaction de lecture : tout échec est propagé.
        let read_txn = match self.db.begin_read() {
            Ok(t) => t,
            Err(e) => {
                return Some(Err(HakilleError::StorageError(format!(
                    "EdgeStore::query_via_index({index_name}): begin_read: {e}"
                ))));
            }
        };

        // 3. Ouverture de l'index : table absente (base fraîche) ⇒ `None`
        //    + warn (fallback légitime) ; toute autre erreur ⇒ propagée.
        let table = match index {
            Index::Spo => read_txn.open_table(SPO_TABLE),
            Index::Pos => read_txn.open_table(POS_TABLE),
            Index::Osp => read_txn.open_table(OSP_TABLE),
        };
        let table = match table {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => {
                tracing::warn!(
                    "EdgeStore::query_via_index({index_name}): table absente (base fraîche) — fallback scan+filtre O(N)"
                );
                return None;
            }
            Err(e) => {
                return Some(Err(HakilleError::StorageError(format!(
                    "EdgeStore::query_via_index({index_name}): open_table: {e}"
                ))));
            }
        };

        // 4. Collecte des ids par préfixe (scan+filtre O(N) — redb sans
        //    prefix-scan natif, voir doc module). Erreur ⇒ propagée.
        let id_bytes_list = match collect_with_prefix(&table, &prefix) {
            Ok(ids) => ids,
            Err(e) => return Some(Err(e)),
        };

        // 5. Table ID : si l'index existe, elle doit exister (écritures
        //    atomiques sur les 4 tables) — absence = corruption ⇒ propagée
        //    (et non retombée en scan aveugle).
        let id_table = match read_txn.open_table(ID_TABLE) {
            Ok(t) => t,
            Err(e) => {
                return Some(Err(HakilleError::StorageError(format!(
                    "EdgeStore::query_via_index({index_name}): index présent mais id_to_triplet illisible ({e}) — corruption ?"
                ))));
            }
        };

        let mut out = Vec::new();
        for id_b in &id_bytes_list {
            let entry = match id_table.get(id_b.as_slice()) {
                Err(e) => {
                    return Some(Err(HakilleError::StorageError(format!(
                        "EdgeStore::query_via_index({index_name}): get id: {e}"
                    ))));
                }
                Ok(None) => {
                    // Index fantôme (id référencé mais triplet absent) :
                    // incohérence signalée, requête poursuivie sur le reste.
                    tracing::warn!(
                        "EdgeStore::query_via_index({index_name}): index fantôme — id sans triplet, entrée ignorée"
                    );
                    continue;
                }
                Ok(Some(entry)) => entry,
            };
            // Échec de décodage (corruption, mauvaise clé de chiffrement)
            // ⇒ propagé : retourner une liste partielle serait un mensonge.
            let triplet = match self.decode_value(entry.value()) {
                Ok(t) => t,
                Err(e) => return Some(Err(e)),
            };
            if pattern_matches(pattern, &triplet) {
                out.push(triplet);
            }
        }
        Some(Ok(out))
    }
}

fn collect_with_prefix(
    table: &redb::ReadOnlyTable<&[u8], &[u8]>,
    prefix: &[u8],
) -> hakille_core::Result<Vec<Vec<u8>>> {
    let mut ids = Vec::new();
    let range = table
        .range(prefix..)
        .map_err(|e| HakilleError::StorageError(format!("EdgeStore: scan index range: {e}")))?;
    for item in range {
        let (k, v) = item.map_err(|e| {
            HakilleError::StorageError(format!("EdgeStore: scan index: lecture entrée: {e}"))
        })?;
        if k.value().starts_with(prefix) {
            ids.push(v.value().to_vec());
        } else {
            break;
        }
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mini-exécuteur synchrone : les futures du store n'attendent jamais
    /// réellement (opérations redb bloquantes enveloppées en `async`), un
    /// simple poll suffit — évite d'ajouter tokio en dépendance de test.
    fn block_on<F: std::future::Future>(mut f: F) -> F::Output {
        use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
        fn clone_raw(_: *const ()) -> RawWaker {
            RawWaker::new(std::ptr::null(), raw_vtable())
        }
        fn noop_raw(_: *const ()) {}
        fn raw_vtable() -> &'static RawWakerVTable {
            &RawWakerVTable::new(clone_raw, noop_raw, noop_raw, noop_raw)
        }
        let waker = unsafe { Waker::from_raw(clone_raw(std::ptr::null())) };
        let mut cx = Context::from_waker(&waker);
        // SAFETY : future épinglé sur la pile, jamais déplacé après pin.
        let mut pinned = unsafe { std::pin::Pin::new_unchecked(&mut f) };
        loop {
            match pinned.as_mut().poll(&mut cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    fn sample_triplet_with(id: u128, subject: u128, label: &str) -> Triplet {
        use crate::triplet::{NodeType, NodeValue, TripletMetadata, TripletSource};
        Triplet {
            id: hakille_core::TripletId(uuid::Uuid::from_u128(id)),
            subject: hakille_core::NodeId(uuid::Uuid::from_u128(subject)),
            predicate: hakille_core::PredicateId(uuid::Uuid::from_u128(0xB0B)),
            object: NodeValue::Text(label.to_string()),
            node_type: Some(NodeType::Fact),
            metadata: TripletMetadata {
                weight: 1.0,
                created_at: hakille_core::Timestamp::now(),
                last_activated: hakille_core::Timestamp::now(),
                source: TripletSource::User,
                confidence: 1.0,
                context: crate::ContextPartition::Personal,
                trust_level: hakille_core::TrustLevel::User,
                decay_rate: 0.01,
            },
        }
    }

    fn sample_triplet() -> Triplet {
        sample_triplet_with(0xC1D, 0xA11CE, "Rust")
    }

    fn open_test_db(name: &str) -> (Arc<Database>, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("hakille-edge-test-{name}-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).expect("création dir temp test");
        let db = redb::Database::create(dir.join("test.redb")).expect("création redb test");
        (Arc::new(db), dir)
    }

    fn table_exists(db: &Database, table: TableDefinition<&[u8], &[u8]>) -> bool {
        let txn = db.begin_read().expect("begin_read test");
        txn.open_table(table).is_ok()
    }

    #[test]
    fn test_edge_index_keys_are_concatenated_and_prefixed() {
        // Verrouille mission 8 : les 3 index existent avec clés concaténées
        // et préfixes exploitables (sujet→SPO, prédicat→POS, objet→OSP).
        let t = sample_triplet();
        let spo = spo_key(&t).unwrap();
        let pos = pos_key(&t).unwrap();
        let osp = osp_key(&t).unwrap();
        assert!(spo.starts_with(&subject_bytes(&t)), "SPO préfixé par sujet");
        assert!(
            pos.starts_with(&predicate_bytes(&t)),
            "POS préfixé par prédicat"
        );
        assert!(
            osp.starts_with(&object_bytes_for_triplet(&t).unwrap()),
            "OSP préfixé par objet"
        );
        assert_ne!(spo, pos, "Index distincts");
        assert_ne!(pos, osp, "Index distincts");
        assert_ne!(spo, osp, "Index distincts");
    }

    #[test]
    fn test_object_bytes_propagate_errors_not_empty() {
        // Pas de clé vide silencieuse : la sérialisation réussit ici et
        // produit des octets non vides.
        let t = sample_triplet();
        let bytes = object_bytes_for_triplet(&t).unwrap();
        assert!(!bytes.is_empty());
        let id_bytes = id_bytes_for_triplet(&t).unwrap();
        assert!(!id_bytes.is_empty());
    }

    #[test]
    fn test_encode_decode_roundtrip_plain_and_encrypted() {
        let t = sample_triplet();
        let dir = std::env::temp_dir().join(format!("hakille-edge-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = redb::Database::create(dir.join("plain.redb")).unwrap();
        let plain = EdgeStore::new(Arc::new(db));
        let bytes = plain.encode_value(&t).unwrap();
        let back = plain.decode_value(&bytes).unwrap();
        assert_eq!(back, t);

        let db2 = redb::Database::create(dir.join("enc.redb")).unwrap();
        let enc = EdgeStore::new(Arc::new(db2)).with_encryption_key([7u8; 32]);
        assert!(enc.has_encryption());
        let enc_bytes = enc.encode_value(&t).unwrap();
        // Chiffré ⇒ différent du clair bincode.
        assert_ne!(enc_bytes, bytes);
        let back2 = enc.decode_value(&enc_bytes).unwrap();
        assert_eq!(back2, t);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_schema_version_written_and_checked() {
        let (db, dir) = open_test_db("schema");
        let store = EdgeStore::try_new(Arc::clone(&db)).expect("try_new base fraîche");
        assert_eq!(block_on(store.count()).expect("count"), 0);

        // Version bien écrite dans META_TABLE.
        {
            let txn = db.begin_read().expect("begin_read");
            let meta = txn.open_table(META_TABLE).expect("meta lisible");
            let guard = meta
                .get(SCHEMA_VERSION_KEY)
                .expect("get version")
                .expect("version présente");
            assert_eq!(
                u32::from_be_bytes([
                    guard.value()[0],
                    guard.value()[1],
                    guard.value()[2],
                    guard.value()[3]
                ]),
                SCHEMA_VERSION
            );
        }

        // Base sabotée (version future) ⇒ try_new refuse explicitement.
        {
            let w = db.begin_write().expect("begin_write sabotage");
            {
                let mut meta = w.open_table(META_TABLE).expect("open meta sabotage");
                meta.insert(SCHEMA_VERSION_KEY, 999u32.to_be_bytes().as_slice())
                    .expect("insert sabotage");
            }
            w.commit().expect("commit sabotage");
        }
        assert!(
            EdgeStore::try_new(Arc::clone(&db)).is_err(),
            "version incompatible doit être refusée"
        );
        drop(store);
        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_batch_store_atomic_and_countable() {
        let (db, dir) = open_test_db("batch");
        let store = EdgeStore::try_new(db).expect("try_new");

        // Lot vide : no-op Ok, rien créé d'utile, count 0.
        block_on(store.batch_store(Vec::new())).expect("batch vide Ok");
        assert_eq!(block_on(store.count()).expect("count"), 0);

        // Lot de 3 : tout ou rien en UNE transaction.
        let lot = vec![
            sample_triplet_with(0xC1D, 0xA11CE, "Rust"),
            sample_triplet_with(0xC1E, 0xA11CE, "Go"),
            sample_triplet_with(0xC1F, 0xBEEF, "Zig"),
        ];
        block_on(store.batch_store(lot.clone())).expect("batch_store");
        assert_eq!(block_on(store.count()).expect("count"), 3);
        // Chaque élément chargeable : aucun état partiel.
        for t in &lot {
            let back = block_on(store.load(&t.id))
                .expect("load")
                .expect("triplet présent après batch");
            assert_eq!(&back, t);
        }
        // Index SPO alimenté par le batch (requête par sujet).
        let pattern = TriplePattern {
            subject: Some(lot[0].subject.clone()),
            predicate: None,
            object: None,
        };
        let hits = block_on(store.query(&pattern)).expect("query sujet");
        assert_eq!(hits.len(), 2, "2 triplets partagent le sujet");

        block_on(store.flush()).expect("flush no-op Ok");
        drop(store);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_delete_noop_creates_no_tables() {
        let (db, dir) = open_test_db("delete-noop");
        let store = EdgeStore::try_new(Arc::clone(&db)).expect("try_new");
        let ghost = hakille_core::TripletId(uuid::Uuid::from_u128(0xDEAD));

        // No-op sur base fraîche : Ok, et AUCUNE table créée.
        block_on(store.delete(&ghost)).expect("delete no-op Ok");
        assert!(!block_on(store.delete_returning(&ghost)).expect("delete_returning"));
        for table in [ID_TABLE, SPO_TABLE, POS_TABLE, OSP_TABLE] {
            assert!(
                !table_exists(&db, table),
                "delete no-op ne doit créer aucune table"
            );
        }

        // Cas réel : store → delete_returning(true) → absent, count 0.
        let t = sample_triplet();
        block_on(store.store(&t)).expect("store");
        assert_eq!(block_on(store.count()).expect("count"), 1);
        assert!(block_on(store.delete_returning(&t.id)).expect("delete"));
        assert_eq!(block_on(store.count()).expect("count"), 0);
        assert!(block_on(store.load(&t.id)).expect("load").is_none());
        // Index nettoyés : requête sujet vide.
        let pattern = TriplePattern {
            subject: Some(t.subject.clone()),
            predicate: None,
            object: None,
        };
        assert!(block_on(store.query(&pattern)).expect("query").is_empty());

        drop(store);
        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_query_propagates_errors_instead_of_swallowing() {
        let (db, dir) = open_test_db("query-err");
        // Écrit chiffré avec KEY1…
        let writer = EdgeStore::try_new(Arc::clone(&db))
            .expect("try_new")
            .with_encryption_key([1u8; 32]);
        let t = sample_triplet();
        block_on(writer.store(&t)).expect("store chiffré");

        // …lu avec une MAUVAISE clé (KEY2) : le déchiffrement échoue.
        let bad_reader = EdgeStore::try_new(Arc::clone(&db))
            .expect("try_new")
            .with_encryption_key([2u8; 32]);
        // Chemin indexé (sujet) : l'erreur DOIT remonter, pas un vec vide.
        let indexed = TriplePattern {
            subject: Some(t.subject.clone()),
            predicate: None,
            object: None,
        };
        assert!(
            block_on(bad_reader.query(&indexed)).is_err(),
            "query indexée avec mauvaise clé doit propager l'erreur (pas de fallback aveugle)"
        );
        // Chemin scan (pattern vide) : idem.
        let scan = TriplePattern {
            subject: None,
            predicate: None,
            object: None,
        };
        assert!(
            block_on(bad_reader.query(&scan)).is_err(),
            "query scan avec mauvaise clé doit propager l'erreur"
        );

        // Contrôle positif : bonne clé ⇒ résultats (indexé + scan).
        let good_reader = EdgeStore::try_new(Arc::clone(&db))
            .expect("try_new")
            .with_encryption_key([1u8; 32]);
        assert_eq!(
            block_on(good_reader.query(&indexed))
                .expect("query ok")
                .len(),
            1
        );
        assert_eq!(
            block_on(good_reader.query(&scan)).expect("scan ok").len(),
            1
        );

        drop(writer);
        drop(bad_reader);
        drop(good_reader);
        drop(db);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_query_fresh_db_falls_back_legitimately() {
        // Base fraîche : pas d'index sélectif / tables absentes ⇒ fallback
        // légitime (warn) avec Ok(vide), pas d'erreur.
        let (db, dir) = open_test_db("fresh-query");
        let store = EdgeStore::try_new(db).expect("try_new");
        let patterns = [
            TriplePattern {
                subject: None,
                predicate: None,
                object: None,
            },
            TriplePattern {
                subject: Some(hakille_core::NodeId(uuid::Uuid::from_u128(0x1))),
                predicate: None,
                object: None,
            },
        ];
        for p in &patterns {
            let hits = block_on(store.query(p)).expect("query base fraîche Ok(vide)");
            assert!(hits.is_empty());
        }
        assert_eq!(block_on(store.count()).expect("count"), 0);
        drop(store);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_prefix_range_scan_pushdown() {
        let (db, dir) = open_test_db("prefix-pushdown");
        let store = EdgeStore::try_new(db).expect("try_new");

        let t1 = sample_triplet_with(1, 0x10, "val1");
        let t2 = sample_triplet_with(2, 0x10, "val2");
        let t3 = sample_triplet_with(3, 0x20, "val3");

        block_on(store.store(&t1)).expect("store t1");
        block_on(store.store(&t2)).expect("store t2");
        block_on(store.store(&t3)).expect("store t3");

        let pattern = TriplePattern {
            subject: Some(hakille_core::NodeId(uuid::Uuid::from_u128(0x10))),
            predicate: Some(hakille_core::PredicateId(uuid::Uuid::from_u128(0xB0B))),
            object: None,
        };

        let hits = block_on(store.query(&pattern)).expect("query");
        assert_eq!(hits.len(), 2);
        assert!(
            hits.iter()
                .all(|t| t.subject == hakille_core::NodeId(uuid::Uuid::from_u128(0x10)))
        );

        drop(store);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_blind_indexing_edge_store_lifecycle_and_zero_knowledge() {
        let (db, dir) = open_test_db("blind-indexing-test");
        let key = [0x42u8; 32];
        let store = EdgeStore::try_new(db.clone())
            .expect("try_new")
            .with_encryption_key(key);

        let t1 = sample_triplet_with(101, 0xAA, "Marie Curie");
        let t2 = sample_triplet_with(102, 0xBB, "Pierre Curie");
        let t3 = sample_triplet_with(103, 0xCC, "Albert Einstein");

        block_on(store.batch_store(vec![t1.clone(), t2.clone(), t3.clone()])).expect("batch_store");

        // 1. Recherche aveugle exacte
        let hits_marie =
            block_on(store.query_blind_exact("marie curie")).expect("query_blind_exact");
        assert_eq!(hits_marie.len(), 1);
        assert_eq!(hits_marie[0].id, t1.id);

        // Tolérance à la casse et espaces
        let hits_marie_caps =
            block_on(store.query_blind_exact("  MARIE   CURIE  ")).expect("query_blind_exact caps");
        assert_eq!(hits_marie_caps.len(), 1);
        assert_eq!(hits_marie_caps[0].id, t1.id);

        // 2. Recherche aveugle par préfixe
        let hits_prefix_curie =
            block_on(store.query_blind_prefix("curie")).expect("query_blind_prefix");
        assert_eq!(hits_prefix_curie.len(), 2);

        let hits_prefix_albert =
            block_on(store.query_blind_prefix("alb")).expect("query_blind_prefix alb");
        assert_eq!(hits_prefix_albert.len(), 1);
        assert_eq!(hits_prefix_albert[0].id, t3.id);

        // 3. Invariant Zero-Knowledge sur disque :
        // Aucun terme en clair ("Marie", "Curie", "Albert") dans les clés blind
        let read_txn = db.begin_read().expect("begin_read");
        let exact_table = read_txn
            .open_table(BLIND_EXACT_TABLE)
            .expect("open blind_exact");
        for item in exact_table.iter().expect("iter") {
            let (token_key, _) = item.expect("entry");
            assert_eq!(token_key.value().len(), 32);
            let raw_key = token_key.value();
            assert!(!raw_key.windows(5).any(|w| w == b"Marie" || w == b"marie"));
            assert!(!raw_key.windows(5).any(|w| w == b"Curie" || w == b"curie"));
        }
        drop(read_txn);

        // 4. Suppression et désindexation
        let deleted = block_on(store.delete_returning(&t1.id)).expect("delete_returning");
        assert!(deleted);

        let hits_after = block_on(store.query_blind_exact("Marie Curie"))
            .expect("query_blind_exact after delete");
        assert!(hits_after.is_empty());

        drop(store);
        std::fs::remove_dir_all(&dir).ok();
    }
}
