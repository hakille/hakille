use crate::triplet::{InsertResult, TriplePattern, Triplet};
use hakille_core::{NodeId, PredicateId, TripletId};
use std::collections::{BTreeMap, HashMap, HashSet};

pub struct KnowledgeGraph {
    /// Stockage principal ordonné (itération déterministe par `TripletId`).
    triplets: BTreeMap<TripletId, Triplet>,
    /// Index RAM `(sujet, prédicat) → IDs` : doublons exacts et routage
    /// contradiction en O(bucket) au lieu de scans O(N).
    by_subject_predicate: HashMap<(NodeId, PredicateId), Vec<TripletId>>,
    /// Index RAM `prédicat → IDs` : requêtes par prédicat seul.
    by_predicate: HashMap<PredicateId, Vec<TripletId>>,
    /// Index RAM `sujet → IDs` : requêtes par sujet seul.
    by_subject: HashMap<NodeId, Vec<TripletId>>,
    subject_names: HashMap<String, NodeId>,
    predicate_names: HashMap<String, PredicateId>,
    subject_labels: HashMap<NodeId, String>,
    predicate_labels: HashMap<PredicateId, String>,
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self {
            triplets: BTreeMap::new(),
            by_subject_predicate: HashMap::new(),
            by_predicate: HashMap::new(),
            by_subject: HashMap::new(),
            subject_names: HashMap::new(),
            predicate_names: HashMap::new(),
            subject_labels: HashMap::new(),
            predicate_labels: HashMap::new(),
        }
    }

    /// Normalisation documentée du registre : suppression des espaces et
    /// passage en minuscules. « Utilisateur » et « utilisateur » désignent
    /// donc la même entité — choix assumé pour un graphe local francophone,
    /// à remplacer par une résolution d'entités si le graphe grandit.
    pub fn normalize_name(name: &str) -> String {
        name.trim().to_lowercase()
    }

    fn fnv1a64(data: &[u8], basis: u64) -> u64 {
        const PRIME: u64 = 1_099_511_628_211;
        let mut hash = basis;
        for b in data {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(PRIME);
        }
        hash
    }

    /// Hash stable documenté (FNV-1a 128 replié en UUID).
    ///
    /// La spec suggérait « SHA-256 tronqué vers UUID v7 » : impossible en
    /// l'état, car un UUID v7 encode timestamp + aléatoire (RFC 9562 §5.7) et
    /// n'offre aucune place pour un condensat. Un UUID v8 « custom » serait
    /// envisageable mais sort du périmètre sans dépendance SHA-256 dans
    /// `hakille-memory`. On utilise donc FNV-1a (déterministe, stable entre
    /// runs, sans dépendance) : même propriété requise — même texte ⇒ même
    /// UUID — avec un risque de collision documenté et négligeable à
    /// l'échelle d'un graphe local. Le registre nom→UUID ci-dessous rend
    /// l'opération réversible pour l'affichage.
    pub fn deterministic_uuid(namespace: &str, name: &str) -> uuid::Uuid {
        let normalized = Self::normalize_name(name);
        let mut h1_input = Vec::with_capacity(namespace.len() + normalized.len() + 1);
        h1_input.extend_from_slice(namespace.as_bytes());
        h1_input.push(0x1F);
        h1_input.extend_from_slice(normalized.as_bytes());
        let mut h2_input = Vec::with_capacity(normalized.len() + namespace.len() + 1);
        h2_input.extend_from_slice(normalized.as_bytes());
        h2_input.push(0x1E);
        h2_input.extend_from_slice(namespace.as_bytes());
        // Bases FNV-1a 64 officielles, seconde base dérivée (golden ratio).
        let h1 = Self::fnv1a64(&h1_input, 14_695_958_103_934_665_637);
        let h2 = Self::fnv1a64(&h2_input, 0x9E37_79B9_7F4A_7C15);
        uuid::Uuid::from_u128(((h1 as u128) << 64) | (h2 as u128))
    }

    /// ID déterministe d'un nœud sujet dérivé de son libellé.
    pub fn subject_id_for_name(name: &str) -> NodeId {
        NodeId(Self::deterministic_uuid("hakille-subject", name))
    }

    /// ID déterministe d'un prédicat dérivé de son libellé.
    pub fn predicate_id_for_name(name: &str) -> PredicateId {
        PredicateId(Self::deterministic_uuid("hakille-predicate", name))
    }

    /// ID déterministe d'un triplet (S/P/O). Permet l'idempotence :
    /// réinsérer le même fait rend le même ID au lieu de dupliquer.
    pub fn deterministic_triplet_id(
        subject: &NodeId,
        predicate: &PredicateId,
        object: &crate::triplet::NodeValue,
    ) -> TripletId {
        let repr = format!("{:?}|{:?}|{:?}", subject.0, predicate.0, object);
        TripletId(Self::deterministic_uuid("hakille-triplet", &repr))
    }

    /// Registre nom→UUID persistant en mémoire du graphe (spec mission 1).
    /// Résout ou crée l'ID déterministe d'un sujet et mémorise le libellé
    /// d'origine pour l'affichage inverse.
    pub fn resolve_subject(&mut self, name: &str) -> NodeId {
        let key = Self::normalize_name(name);
        if let Some(id) = self.subject_names.get(&key) {
            return id.clone();
        }
        let id = Self::subject_id_for_name(name);
        self.subject_names.insert(key.clone(), id.clone());
        self.subject_labels
            .entry(id.clone())
            .or_insert_with(|| name.trim().to_string());
        id
    }

    /// Registre nom→UUID persistant en mémoire du graphe (prédicats).
    pub fn resolve_predicate(&mut self, name: &str) -> PredicateId {
        let key = Self::normalize_name(name);
        if let Some(id) = self.predicate_names.get(&key) {
            return id.clone();
        }
        let id = Self::predicate_id_for_name(name);
        self.predicate_names.insert(key.clone(), id.clone());
        self.predicate_labels
            .entry(id.clone())
            .or_insert_with(|| name.trim().to_string());
        id
    }

    pub fn lookup_subject(&self, name: &str) -> Option<NodeId> {
        self.subject_names.get(&Self::normalize_name(name)).cloned()
    }

    pub fn lookup_predicate(&self, name: &str) -> Option<PredicateId> {
        self.predicate_names
            .get(&Self::normalize_name(name))
            .cloned()
    }

    pub fn subject_label(&self, id: &NodeId) -> Option<&str> {
        self.subject_labels.get(id).map(|s| s.as_str())
    }

    pub fn predicate_label(&self, id: &PredicateId) -> Option<&str> {
        self.predicate_labels.get(id).map(|s| s.as_str())
    }

    // ------------------------------------------------------------------
    // Index RAM (privés, maintenus à chaque mutation).
    // ------------------------------------------------------------------

    /// Indexe un triplet dans les 3 registres (garde anti-doublon : un ID
    /// déjà présent dans le bucket n'est pas ré-additionné).
    fn index_insert(&mut self, subject: &NodeId, predicate: &PredicateId, id: &TripletId) {
        let bucket = self
            .by_subject_predicate
            .entry((subject.clone(), predicate.clone()))
            .or_default();
        if !bucket.contains(id) {
            bucket.push(id.clone());
        }
        let bucket = self.by_subject.entry(subject.clone()).or_default();
        if !bucket.contains(id) {
            bucket.push(id.clone());
        }
        let bucket = self.by_predicate.entry(predicate.clone()).or_default();
        if !bucket.contains(id) {
            bucket.push(id.clone());
        }
    }

    /// Désindexe un triplet ; les buckets vidés sont supprimés (pas de
    /// vecteurs fantômes qui gonfleraient `query`).
    fn index_remove(&mut self, subject: &NodeId, predicate: &PredicateId, id: &TripletId) {
        let sp = (subject.clone(), predicate.clone());
        remove_id_from_bucket(self.by_subject_predicate.get_mut(&sp), id);
        if self
            .by_subject_predicate
            .get(&sp)
            .is_some_and(|bucket| bucket.is_empty())
        {
            self.by_subject_predicate.remove(&sp);
        }
        remove_id_from_bucket(self.by_subject.get_mut(subject), id);
        if self
            .by_subject
            .get(subject)
            .is_some_and(|bucket| bucket.is_empty())
        {
            self.by_subject.remove(subject);
        }
        remove_id_from_bucket(self.by_predicate.get_mut(predicate), id);
        if self
            .by_predicate
            .get(predicate)
            .is_some_and(|bucket| bucket.is_empty())
        {
            self.by_predicate.remove(predicate);
        }
    }

    /// Reconstruit les 3 index depuis le `BTreeMap` principal (utilisé après
    /// `hydrate_from` / `load_from_file`, qui chargent en masse).
    fn rebuild_indexes(&mut self) {
        self.by_subject_predicate.clear();
        self.by_subject.clear();
        self.by_predicate.clear();
        let ids: Vec<(NodeId, PredicateId, TripletId)> = self
            .triplets
            .values()
            .map(|t| (t.subject.clone(), t.predicate.clone(), t.id.clone()))
            .collect();
        for (subject, predicate, id) in &ids {
            self.index_insert(subject, predicate, id);
        }
    }

    /// Stocke un triplet en tenant les index (écrase à ID égal après purge
    /// de l'ancien indexage — pas d'entrées fantômes en cas de réinsertion
    /// du même ID avec un contenu différent).
    fn store_new(&mut self, triplet: Triplet) -> TripletId {
        if self.triplets.contains_key(&triplet.id)
            && let Some(old) = self.triplets.remove(&triplet.id)
        {
            self.index_remove(&old.subject, &old.predicate, &old.id);
        }
        let id = triplet.id.clone();
        let subject = triplet.subject.clone();
        let predicate = triplet.predicate.clone();
        self.triplets.insert(id.clone(), triplet);
        self.index_insert(&subject, &predicate, &id);
        id
    }

    /// Insertion avec routage contradiction (spec §8, mission 2).
    ///
    /// - Doublon exact S/P/O ⇒ idempotent : réactive l'existant et rend
    ///   `Inserted(id_existant)` sans dupliquer. La détection passe par
    ///   l'index `(S,P)` (O(bucket)) au lieu du scan O(N) historique.
    /// - Même S/P + objet différent (ou écart de confiance) ⇒
    ///   `detect_contradiction` + `resolve` :
    ///   - `ReplaceWithNew` ⇒ l'ancien est retiré, le nouveau inséré,
    ///     retour `ContradictionResolved { old, new }` (traçabilité).
    ///   - `AskUser` ⇒ `NeedsUserResolution { existing, incoming }` SANS
    ///     écraser (jamais silencieux).
    /// - Sinon insertion indexée.
    ///
    /// Multi-conflits : les candidats sont examinés dans l'ordre `BTreeMap`
    /// (tri par `TripletId`, déterministe). Le premier `AskUser` gagne ;
    /// sinon tous les remplaçables sont retirés, le premier `old` (ordre
    /// `TripletId`) est rapporté et les autres sont tracés via
    /// `tracing::warn!` (jamais d'écrasement muet).
    pub fn insert(&mut self, triplet: Triplet) -> InsertResult {
        let sp_key = (triplet.subject.clone(), triplet.predicate.clone());

        // Doublon exact S/P/O via l'index (O(bucket)).
        if let Some(bucket) = self.by_subject_predicate.get(&sp_key) {
            let bucket = bucket.clone();
            if let Some(existing_id) = bucket
                .iter()
                .filter_map(|id| self.triplets.get(id))
                .find(|t| t.object == triplet.object)
                .map(|t| t.id.clone())
            {
                self.activate(&existing_id);
                return InsertResult::Inserted(existing_id);
            }
        }

        // Candidats contradiction : bucket (S,P), ordre TripletId déterministe.
        let mut candidate_ids: Vec<TripletId> = self
            .by_subject_predicate
            .get(&sp_key)
            .cloned()
            .unwrap_or_default();
        candidate_ids.sort();
        let mut replace_ids: Vec<TripletId> = Vec::new();
        for candidate_id in &candidate_ids {
            let existing = match self.triplets.get(candidate_id) {
                Some(t) => t.clone(),
                None => continue,
            };
            if crate::contradiction::detect_contradiction(&existing, &triplet).is_none() {
                continue;
            }
            match crate::contradiction::resolve(&existing, &triplet) {
                crate::contradiction::Resolution::ReplaceWithNew => {
                    replace_ids.push(existing.id.clone());
                }
                crate::contradiction::Resolution::AskUser { .. } => {
                    return InsertResult::NeedsUserResolution {
                        existing: existing.clone(),
                        incoming: triplet,
                    };
                }
                crate::contradiction::Resolution::KeepExisting => {}
            }
        }

        if let Some(first_old) = replace_ids.first().cloned() {
            if replace_ids.len() > 1 {
                tracing::warn!(
                    "KnowledgeGraph::insert : {} conflits remplaçables sur le même (sujet, prédicat) ; \
                     remplacement rapporté pour {:?}, autres IDs écrasés : {:?}",
                    replace_ids.len(),
                    first_old,
                    &replace_ids[1..]
                );
            }
            for old in &replace_ids {
                if let Some(removed) = self.triplets.remove(old) {
                    self.index_remove(&removed.subject, &removed.predicate, &removed.id);
                }
            }
            let new_id = self.store_new(triplet);
            return InsertResult::ContradictionResolved {
                old: first_old,
                new: new_id,
            };
        }

        let id = self.store_new(triplet);
        InsertResult::Inserted(id)
    }

    /// Requête par motif, routée vers l'index RAM le plus sélectif :
    /// - sujet + prédicat ⇒ index `(S,P)` ;
    /// - sujet seul ⇒ index sujet ;
    /// - prédicat seul ⇒ index prédicat ;
    /// - objet seul ou motif vide ⇒ scan `BTreeMap` (pas d'index objet en
    ///   RAM : l'objet `NodeValue` n'offre pas de clé `Hash` stable et bon
    ///   marché — le tier edge redb couvre ce cas via son index OSP).
    ///
    /// Résultats triés par `TripletId` (ordre total déterministe, identique
    /// au scan historique).
    pub fn query(&self, pattern: &TriplePattern) -> Vec<&Triplet> {
        let indexed: Option<&Vec<TripletId>> = match (&pattern.subject, &pattern.predicate) {
            (Some(s), Some(p)) => self.by_subject_predicate.get(&(s.clone(), p.clone())),
            (Some(s), None) => self.by_subject.get(s),
            (None, Some(p)) => self.by_predicate.get(p),
            (None, None) => None,
        };
        let mut out: Vec<&Triplet> = match indexed {
            Some(ids) => ids
                .iter()
                .filter_map(|id| self.triplets.get(id))
                .filter(|t| pattern.object.as_ref().is_none_or(|o| o == &t.object))
                .collect(),
            None => self
                .triplets
                .values()
                .filter(|t| {
                    let s_match = pattern.subject.as_ref().is_none_or(|s| s == &t.subject);
                    let p_match = pattern.predicate.as_ref().is_none_or(|p| p == &t.predicate);
                    let o_match = pattern.object.as_ref().is_none_or(|o| o == &t.object);
                    s_match && p_match && o_match
                })
                .collect(),
        };
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    pub fn len(&self) -> usize {
        self.triplets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.triplets.is_empty()
    }

    pub fn all_triplets(&self) -> Vec<&Triplet> {
        self.triplets.values().collect()
    }

    /// IDs de tous les triplets (support de la décroissance temporelle).
    pub fn ids(&self) -> Vec<TripletId> {
        self.triplets.keys().cloned().collect()
    }

    /// Accès mutable ciblé (support de la décroissance temporelle).
    pub fn get_mut(&mut self, id: &TripletId) -> Option<&mut Triplet> {
        self.triplets.get_mut(id)
    }

    pub fn search_text(&self, query: &str) -> Vec<&Triplet> {
        let q = query.to_lowercase();
        self.triplets
            .values()
            .filter(|t| match &t.object {
                crate::triplet::NodeValue::Text(s) => s.to_lowercase().contains(&q),
                crate::triplet::NodeValue::Entity(_) => false,
                crate::triplet::NodeValue::Number(n) => n.to_string().contains(&q),
                crate::triplet::NodeValue::Boolean(b) => b.to_string().contains(&q),
                _ => false,
            })
            .collect()
    }

    pub fn get(&self, id: &TripletId) -> Option<&Triplet> {
        self.triplets.get(id)
    }

    /// Parcours BFS depuis `node` jusqu'à `max_depth`, déterministe et sans
    /// doublons : les triplets déjà émis (suivis par ID — un même triplet
    /// est atteignable côté sujet ET côté objet entité) ne sont rendus
    /// qu'une fois, et le résultat final est trié par poids décroissant
    /// (égalité ⇒ `TripletId`, ordre total stable entre runs).
    pub fn traverse_from(&self, node: &NodeId, max_depth: usize) -> Vec<&Triplet> {
        let mut visited_nodes = HashSet::new();
        let mut visited_triplets = HashSet::new();
        let mut result = Vec::new();
        let mut current_layer = vec![node.clone()];
        visited_nodes.insert(node.clone());

        for _ in 0..max_depth {
            if current_layer.is_empty() {
                break;
            }
            current_layer.sort();
            let mut next_layer = Vec::new();
            for current in &current_layer {
                for triplet in self.related(current) {
                    if visited_triplets.insert(triplet.id.clone()) {
                        result.push(triplet);
                    }
                    if let crate::triplet::NodeValue::Entity(ref target) = triplet.object
                        && visited_nodes.insert(target.clone())
                    {
                        next_layer.push(target.clone());
                    }
                }
            }
            current_layer = next_layer;
        }

        sort_by_weight_desc(&mut result);
        result
    }

    /// Triplets impliquant `node` (côté sujet ou côté objet entité), triés
    /// par poids décroissant (égalité ⇒ `TripletId`) pour un ordre
    /// déterministe.
    pub fn related(&self, node: &NodeId) -> Vec<&Triplet> {
        let mut out: Vec<&Triplet> = self
            .triplets
            .values()
            .filter(|t| {
                &t.subject == node
                    || match &t.object {
                        crate::triplet::NodeValue::Entity(n) => n == node,
                        _ => false,
                    }
            })
            .collect();
        sort_by_weight_desc(&mut out);
        out
    }

    /// Réactivation d'un souvenir : `last_activated` passe à maintenant et le
    /// poids est renforcé (`weight * 1.1`, plafonné à `1.0`). ID inconnu ⇒
    /// sans effet (pas d'erreur : l'activation est une suggestion, pas un
    /// accès).
    pub fn activate(&mut self, id: &TripletId) {
        if let Some(t) = self.triplets.get_mut(id) {
            t.metadata.last_activated = hakille_core::Timestamp::now();
            t.metadata.weight = (t.metadata.weight * 1.1).min(1.0);
        }
    }

    /// Retire un triplet et purge ses 3 entrées d'index (pas de buckets
    /// fantômes). Rend `false` si l'ID était absent.
    pub fn remove(&mut self, id: &TripletId) -> bool {
        if let Some(removed) = self.triplets.remove(id) {
            self.index_remove(&removed.subject, &removed.predicate, &removed.id);
            true
        } else {
            false
        }
    }

    pub fn contains(&self, id: &TripletId) -> bool {
        self.triplets.contains_key(id)
    }

    /// Persiste tout le graphe RAM vers le tier edge redb (`EdgeStore`).
    ///
    /// Itère `all_triplets` et `store` chacun (écrase à l'identique :
    /// idempotent). Requiert `crate::storage::GraphStorage` en portée côté
    /// appelant pour le trait, mais pas ici (appel direct via le trait).
    /// Ne supprime RIEN côté base : les IDs supprimés en RAM restent en base
    /// (fantômes) — voir [`Self::persist_to_with_gc`] pour une persistance
    /// avec ramasse-miettes.
    pub async fn persist_to(
        &self,
        edge: &crate::storage::edge::EdgeStore,
    ) -> hakille_core::Result<()> {
        use crate::storage::GraphStorage;
        for t in self.triplets.values() {
            edge.store(t).await?;
        }
        Ok(())
    }

    /// Persiste le graphe RAM vers redb PUIS supprime de la base les triplets
    /// présents en base mais absents en RAM (ramasse-miettes anti-fantômes :
    /// un `remove` en RAM suivi d'un `persist` simple laissait l'ID en base,
    /// qui ressuscitait au prochain `hydrate_from`).
    pub async fn persist_to_with_gc(
        &self,
        edge: &crate::storage::edge::EdgeStore,
    ) -> hakille_core::Result<()> {
        use crate::storage::GraphStorage;
        for t in self.triplets.values() {
            edge.store(t).await?;
        }
        let stored = edge.all_triplets().await?;
        for t in stored {
            if !self.triplets.contains_key(&t.id) {
                edge.delete(&t.id).await?;
            }
        }
        Ok(())
    }

    /// Hydrate le graphe RAM depuis le tier edge redb (`EdgeStore`).
    ///
    /// Vide l'état courant puis charge `all_triplets` (insertion directe,
    /// sans routage contradiction, pour une restauration fidèle),
    /// reconstruit les 3 index RAM et RECONSTRUIT les registres
    /// nom→ID/libellés à partir des triplets chargés (correctif du bug
    /// critique : l'ancienne version laissait des registres vides, rendant
    /// `lookup_*`/`resolve_*` incohérents après hydration).
    ///
    /// Reconstruction des registres (meilleur effort, documenté) : les UUID
    /// étant dérivés par hash irréversible, les libellés d'origine sont
    /// perdus si la cible ne les connaît pas déjà. Règles appliquées :
    /// - les libellés déjà connus pour un ID toujours présent sont
    ///   conservés (ex. `resolve_subject("Alice")` avant `hydrate_from`) ;
    /// - tout ID sans libellé reçoit l'UUID en clair comme étiquette de
    ///   repli (recherche par cet UUID possible via `lookup_*`) ;
    /// - les entrées (noms et libellés) pointant vers des IDs absents des
    ///   triplets chargés sont purgées (pas de registres fantômes).
    ///
    /// Pour une fidélité totale des libellés, préférer `save_to_file` /
    /// `load_from_file` (snapshot JSON incluant les registres).
    pub async fn hydrate_from(
        &mut self,
        edge: &crate::storage::edge::EdgeStore,
    ) -> hakille_core::Result<()> {
        use crate::storage::GraphStorage;
        let triplets = edge.all_triplets().await?;
        self.triplets.clear();
        for t in triplets {
            self.triplets.insert(t.id.clone(), t);
        }
        self.rebuild_indexes();
        self.rebuild_registries_from_triplets();
        Ok(())
    }

    /// Reconstruit les registres nom→ID/libellés depuis les triplets
    /// présents en RAM (voir [`Self::hydrate_from`] pour les règles).
    fn rebuild_registries_from_triplets(&mut self) {
        let mut subjects = HashSet::new();
        let mut predicates = HashSet::new();
        for t in self.triplets.values() {
            subjects.insert(t.subject.clone());
            predicates.insert(t.predicate.clone());
        }
        // Purge des libellés pointant vers des IDs disparus.
        self.subject_labels.retain(|id, _| subjects.contains(id));
        self.predicate_labels
            .retain(|id, _| predicates.contains(id));
        // Repli : UUID en clair pour tout ID sans libellé connu.
        for id in &subjects {
            self.subject_labels
                .entry(id.clone())
                .or_insert_with(|| id.0.to_string());
        }
        for id in &predicates {
            self.predicate_labels
                .entry(id.clone())
                .or_insert_with(|| id.0.to_string());
        }
        // Registres nom→ID dérivés des libellés (conservés ou replis).
        self.subject_names.clear();
        for (id, label) in &self.subject_labels {
            self.subject_names
                .insert(Self::normalize_name(label), id.clone());
        }
        self.predicate_names.clear();
        for (id, label) in &self.predicate_labels {
            self.predicate_names
                .insert(Self::normalize_name(label), id.clone());
        }
    }

    /// Sauvegarde JSON atomique vers un fichier (triplets + registres).
    ///
    /// Le snapshot est écrit dans un fichier temporaire du même répertoire,
    /// synchronisé sur disque, puis renommé sur la destination. Un crash
    /// pendant l'écriture laisse donc l'ancien snapshot intact, jamais un
    /// JSON tronqué qui empêcherait le prochain démarrage.
    pub fn save_to_file(&self, path: impl AsRef<std::path::Path>) -> hakille_core::Result<()> {
        use std::io::Write as _;

        let path = path.as_ref();
        let snapshot = GraphSnapshot {
            triplets: self.triplets.values().cloned().collect(),
            subject_names: self
                .subject_names
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            predicate_names: self
                .predicate_names
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            subject_labels: self
                .subject_labels
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            predicate_labels: self
                .predicate_labels
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        };
        let bytes = serde_json::to_vec_pretty(&snapshot)
            .map_err(|e| hakille_core::HakilleError::SerializationError(e.to_string()))?;

        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .map_err(|e| hakille_core::HakilleError::StorageError(e.to_string()))?;
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                hakille_core::HakilleError::StorageError(format!(
                    "chemin de sauvegarde invalide: {}",
                    path.display()
                ))
            })?;
        static TEMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nonce = TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let temp_path =
            path.with_file_name(format!(".{file_name}.tmp-{}-{nonce}", std::process::id()));

        let write_result = (|| -> std::io::Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temp_path, path)?;
            // Rendre le rename durable sur les systèmes POSIX. Sur Windows,
            // `rename` est déjà une opération de répertoire atomique.
            #[cfg(unix)]
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
                && let Ok(dir) = std::fs::File::open(parent)
            {
                dir.sync_all()?;
            }
            Ok(())
        })();

        if let Err(e) = write_result {
            let _ = std::fs::remove_file(&temp_path);
            return Err(hakille_core::HakilleError::StorageError(format!(
                "sauvegarde atomique de {} impossible: {e}",
                path.display()
            )));
        }
        Ok(())
    }

    /// Charge un graphe depuis un fichier JSON écrit par [`Self::save_to_file`].
    /// Les index RAM sont reconstruits depuis les triplets chargés.
    pub fn load_from_file(path: impl AsRef<std::path::Path>) -> hakille_core::Result<Self> {
        let bytes = std::fs::read(path.as_ref())
            .map_err(|e| hakille_core::HakilleError::StorageError(e.to_string()))?;
        let snapshot: GraphSnapshot = serde_json::from_slice(&bytes)
            .map_err(|e| hakille_core::HakilleError::SerializationError(e.to_string()))?;
        let mut graph = Self::new();
        for t in snapshot.triplets {
            graph.triplets.insert(t.id.clone(), t);
        }
        graph.rebuild_indexes();
        graph.subject_names = snapshot.subject_names.into_iter().collect();
        graph.predicate_names = snapshot.predicate_names.into_iter().collect();
        graph.subject_labels = snapshot.subject_labels.into_iter().collect();
        graph.predicate_labels = snapshot.predicate_labels.into_iter().collect();
        Ok(graph)
    }
}

/// Retire `id` d'un bucket d'index en O(1) (`swap_remove` — l'ordre des
/// buckets n'est pas significatif, les lectures trient par `TripletId`).
fn remove_id_from_bucket(bucket: Option<&mut Vec<TripletId>>, id: &TripletId) {
    if let Some(bucket) = bucket
        && let Some(pos) = bucket.iter().position(|x| x == id)
    {
        bucket.swap_remove(pos);
    }
}

/// Tri par poids décroissant, égalité départagée par `TripletId` (ordre total
/// déterministe ; poids `NaN` traités comme égaux puis départagés par ID).
fn sort_by_weight_desc(items: &mut Vec<&Triplet>) {
    items.sort_by(|a, b| {
        b.metadata
            .weight
            .partial_cmp(&a.metadata.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// Snapshot JSON du graphe (triplets + registres nom↔ID/libellés).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphSnapshot {
    pub triplets: Vec<crate::triplet::Triplet>,
    pub subject_names: Vec<(String, NodeId)>,
    pub predicate_names: Vec<(String, PredicateId)>,
    pub subject_labels: Vec<(NodeId, String)>,
    pub predicate_labels: Vec<(PredicateId, String)>,
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_partition::ContextPartition;
    use crate::triplet::{NodeType, NodeValue, TripletMetadata, TripletSource};
    use hakille_core::{Timestamp, TrustLevel};
    use std::sync::Arc;

    fn sample_triplet(
        subject: &NodeId,
        predicate: &PredicateId,
        object: NodeValue,
        weight: f64,
        confidence: f64,
    ) -> Triplet {
        Triplet {
            id: TripletId(uuid::Uuid::now_v7()),
            subject: subject.clone(),
            predicate: predicate.clone(),
            object,
            metadata: TripletMetadata {
                weight,
                created_at: Timestamp::now(),
                last_activated: Timestamp::now(),
                source: TripletSource::User,
                confidence,
                context: ContextPartition::Default,
                trust_level: TrustLevel::User,
                decay_rate: 0.01,
            },
            node_type: Some(NodeType::Fact),
        }
    }

    fn fresh_ids() -> (NodeId, PredicateId) {
        (
            NodeId(uuid::Uuid::now_v7()),
            PredicateId(uuid::Uuid::now_v7()),
        )
    }

    fn temp_edge(name: &str) -> (crate::storage::edge::EdgeStore, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("hakille-graph-{}-{}", name, uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = redb::Database::create(dir.join("test.redb")).unwrap();
        (crate::storage::edge::EdgeStore::new(Arc::new(db)), dir)
    }

    #[test]
    fn test_ram_index_insert_query_and_duplicate() {
        let mut graph = KnowledgeGraph::new();
        let (s1, p1) = fresh_ids();
        let (s2, p2) = (
            NodeId(uuid::Uuid::now_v7()),
            PredicateId(uuid::Uuid::now_v7()),
        );

        let t1 = sample_triplet(&s1, &p1, NodeValue::Text("x".into()), 0.5, 0.9);
        let t2 = sample_triplet(&s1, &p2, NodeValue::Text("y".into()), 0.6, 0.9);
        let t3 = sample_triplet(&s2, &p1, NodeValue::Text("z".into()), 0.7, 0.9);
        let id1 = t1.id.clone();
        assert!(matches!(graph.insert(t1), InsertResult::Inserted(id) if id == id1));
        graph.insert(t2);
        graph.insert(t3);
        assert_eq!(graph.len(), 3);

        // Index peuplés (3 buckets (S,P) ; sujets s1→2/s2→1 ; prédicats p1→2/p2→1).
        assert_eq!(graph.by_subject_predicate.len(), 3);
        assert_eq!(graph.by_subject.get(&s1).unwrap().len(), 2);
        assert_eq!(graph.by_subject.get(&s2).unwrap().len(), 1);
        assert_eq!(graph.by_predicate.get(&p1).unwrap().len(), 2);
        assert_eq!(graph.by_predicate.get(&p2).unwrap().len(), 1);

        // Requêtes routées index : sujet seul, prédicat seul, (S,P), motif vide.
        let by_s1 = graph.query(&TriplePattern {
            subject: Some(s1.clone()),
            predicate: None,
            object: None,
        });
        assert_eq!(by_s1.len(), 2);
        let by_p1 = graph.query(&TriplePattern {
            subject: None,
            predicate: Some(p1.clone()),
            object: None,
        });
        assert_eq!(by_p1.len(), 2);
        let by_sp = graph.query(&TriplePattern {
            subject: Some(s1.clone()),
            predicate: Some(p1.clone()),
            object: None,
        });
        assert_eq!(by_sp.len(), 1);
        assert_eq!(by_sp[0].id, id1);
        let all = graph.query(&TriplePattern {
            subject: None,
            predicate: None,
            object: None,
        });
        assert_eq!(all.len(), 3);

        // Doublon exact S/P/O avec un NOUVEL id : idempotent via l'index.
        let dup = sample_triplet(&s1, &p1, NodeValue::Text("x".into()), 0.5, 0.9);
        match graph.insert(dup) {
            InsertResult::Inserted(id) => assert_eq!(id, id1),
            other => panic!("doublon attendu idempotent, obtenu : {:?}", other),
        }
        assert_eq!(graph.len(), 3);
        assert_eq!(
            graph
                .by_subject_predicate
                .get(&(s1.clone(), p1.clone()))
                .unwrap()
                .len(),
            1
        );

        // `remove` purge les index (pas de buckets fantômes).
        assert!(graph.remove(&id1));
        assert_eq!(graph.len(), 2);
        assert!(!graph.by_subject_predicate.contains_key(&(s1, p1)));
        assert_eq!(graph.by_subject.get(&s2).unwrap().len(), 1);
    }

    #[test]
    fn test_activate_boosts_weight_and_refreshes_timestamp() {
        let mut graph = KnowledgeGraph::new();
        let (s, p) = fresh_ids();
        let mut t = sample_triplet(&s, &p, NodeValue::Text("souvenir".into()), 0.5, 0.9);
        t.metadata.last_activated = Timestamp(chrono::Utc::now() - chrono::Duration::hours(1));
        let before = t.metadata.last_activated.clone();
        let id = t.id.clone();
        graph.insert(t);

        graph.activate(&id);
        let got = graph.get(&id).unwrap();
        assert!((got.metadata.weight - 0.55).abs() < 1e-12);
        assert!(got.metadata.last_activated.0 >= before.0);

        // Plafond à 1.0.
        graph.get_mut(&id).unwrap().metadata.weight = 0.99;
        graph.activate(&id);
        assert!((graph.get(&id).unwrap().metadata.weight - 1.0).abs() < 1e-12);

        // ID inconnu : sans effet, sans panique.
        graph.activate(&TripletId(uuid::Uuid::now_v7()));
    }

    #[test]
    fn test_traverse_from_deterministic_without_duplicates() {
        let mut graph = KnowledgeGraph::new();
        let a = NodeId(uuid::Uuid::now_v7());
        let b = NodeId(uuid::Uuid::now_v7());
        let p = PredicateId(uuid::Uuid::now_v7());
        // Cycle A→B→A : chaque triplet est atteignable des deux côtés
        // (sujet + objet entité) — sans déduplication on rendrait 4 entrées.
        let t1 = sample_triplet(&a, &p, NodeValue::Entity(b.clone()), 0.9, 0.9);
        let t2 = sample_triplet(&b, &p, NodeValue::Entity(a.clone()), 0.3, 0.9);
        let id1 = t1.id.clone();
        let id2 = t2.id.clone();
        graph.insert(t1);
        graph.insert(t2);

        let first = graph.traverse_from(&a, 5);
        assert_eq!(first.len(), 2, "doublons non dédupliqués");
        // Tri par poids décroissant.
        assert_eq!(first[0].id, id1);
        assert_eq!(first[1].id, id2);
        // Déterminisme entre runs.
        let second = graph.traverse_from(&a, 5);
        assert_eq!(
            first.iter().map(|t| &t.id).collect::<Vec<_>>(),
            second.iter().map(|t| &t.id).collect::<Vec<_>>()
        );
        // Profondeur 0 : rien.
        assert!(graph.traverse_from(&a, 0).is_empty());
        // `related` trié par poids décroissant lui aussi.
        let rel = graph.related(&a);
        assert_eq!(rel.len(), 2);
        assert_eq!(rel[0].id, id1);
    }

    #[test]
    fn test_insert_multi_replace_returns_first_old() {
        let mut graph = KnowledgeGraph::new();
        let (s, p) = fresh_ids();
        // Deux faits coexistants de même (S,P), objets différents, confiances
        // faibles (< 0.5 ⇒ pas de contradiction à l'insertion) : on les pose
        // directement pour simuler un historique pré-routage.
        let mut e1 = sample_triplet(&s, &p, NodeValue::Text("Paris".into()), 0.5, 0.4);
        let mut e2 = sample_triplet(&s, &p, NodeValue::Text("Lyon".into()), 0.5, 0.4);
        if e1.id > e2.id {
            std::mem::swap(&mut e1.id, &mut e2.id);
        }
        let expected_old = e1.id.clone();
        graph.triplets.insert(e1.id.clone(), e1);
        graph.triplets.insert(e2.id.clone(), e2);
        graph.rebuild_indexes();

        // Entrant nettement plus sûr : remplace les DEUX (gap >= 0.2 chacun).
        let incoming = sample_triplet(&s, &p, NodeValue::Text("Dakar".into()), 0.5, 0.95);
        let new_id = incoming.id.clone();
        match graph.insert(incoming) {
            InsertResult::ContradictionResolved { old, new } => {
                assert_eq!(
                    old, expected_old,
                    "le premier old (ordre TripletId) doit être rapporté"
                );
                assert_eq!(new, new_id);
            }
            other => panic!("remplacement multi-conflits attendu, obtenu : {:?}", other),
        }
        assert_eq!(graph.len(), 1);
        assert!(graph.contains(&new_id));
    }

    #[tokio::test]
    async fn test_hydrate_rebuilds_indexes_and_registries() {
        let (edge, dir) = temp_edge("hydrate");

        // Graphe source avec vrais libellés résolus.
        let mut source = KnowledgeGraph::new();
        let alice = source.resolve_subject("Alice");
        let aime = source.resolve_predicate("aime");
        let t = sample_triplet(&alice, &aime, NodeValue::Text("le café".into()), 0.8, 0.9);
        let tid = t.id.clone();
        source.insert(t);
        source.persist_to(&edge).await.unwrap();

        // Hydration dans un graphe pré-ensemencé avec les mêmes noms :
        // les libellés réels sont conservés (mêmes IDs déterministes).
        let mut seeded = KnowledgeGraph::new();
        seeded.resolve_subject("Alice");
        seeded.resolve_predicate("aime");
        seeded.hydrate_from(&edge).await.unwrap();
        assert_eq!(seeded.len(), 1);
        assert!(seeded.contains(&tid));
        assert_eq!(seeded.subject_label(&alice), Some("Alice"));
        assert_eq!(seeded.predicate_label(&aime), Some("aime"));
        assert_eq!(seeded.lookup_subject("alice"), Some(alice.clone()));
        // Index reconstruits.
        assert_eq!(seeded.by_subject_predicate.len(), 1);
        assert_eq!(seeded.by_subject.get(&alice).unwrap().len(), 1);
        assert_eq!(seeded.by_predicate.get(&aime).unwrap().len(), 1);
        assert_eq!(
            seeded
                .query(&TriplePattern {
                    subject: Some(alice.clone()),
                    predicate: Some(aime.clone()),
                    object: None
                })
                .len(),
            1
        );

        // Hydration dans un graphe vierge : repli UUID documenté.
        let mut fresh = KnowledgeGraph::new();
        fresh.hydrate_from(&edge).await.unwrap();
        assert_eq!(fresh.len(), 1);
        let fallback = alice.0.to_string();
        assert_eq!(fresh.subject_label(&alice), Some(fallback.as_str()));
        assert_eq!(fresh.lookup_subject(&fallback), Some(alice.clone()));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_persist_with_gc_removes_ghosts() {
        use crate::storage::GraphStorage;
        let (edge, dir) = temp_edge("gc");

        let mut graph = KnowledgeGraph::new();
        let (s1, p1) = fresh_ids();
        let (s2, p2) = fresh_ids();
        let keep = sample_triplet(&s1, &p1, NodeValue::Text("gardé".into()), 0.8, 0.9);
        let ghost = sample_triplet(&s2, &p2, NodeValue::Text("fantôme".into()), 0.8, 0.9);
        let keep_id = keep.id.clone();
        let ghost_id = ghost.id.clone();
        graph.insert(keep);
        graph.insert(ghost);
        graph.persist_to(&edge).await.unwrap();
        assert_eq!(edge.all_triplets().await.unwrap().len(), 2);

        // `persist_to` simple ne ramasse rien : le fantôme survit en base.
        assert!(graph.remove(&ghost_id));
        graph.persist_to(&edge).await.unwrap();
        assert_eq!(edge.all_triplets().await.unwrap().len(), 2);

        // `persist_to_with_gc` supprime de redb les IDs absents de la RAM.
        graph.persist_to_with_gc(&edge).await.unwrap();
        let remaining = edge.all_triplets().await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, keep_id);

        // Compat : `persist_to` existe toujours et persiste tout le RAM.
        graph.persist_to(&edge).await.unwrap();
        assert_eq!(edge.all_triplets().await.unwrap().len(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_to_file_roundtrip_uses_atomic_replacement() {
        let dir = std::env::temp_dir().join(format!(
            "hakille-graph-snapshot-{}-{}",
            std::process::id(),
            uuid::Uuid::now_v7()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("memory.json");

        let mut graph = KnowledgeGraph::new();
        let alice = graph.resolve_subject("Alice");
        let aime = graph.resolve_predicate("aime");
        let triplet = sample_triplet(&alice, &aime, NodeValue::Text("le café".into()), 0.8, 0.9);
        let triplet_id = triplet.id.clone();
        graph.insert(triplet);

        graph.save_to_file(&path).unwrap();
        assert!(path.exists(), "le snapshot final doit exister");
        let loaded = KnowledgeGraph::load_from_file(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(loaded.contains(&triplet_id));
        assert_eq!(loaded.subject_label(&alice), Some("Alice"));
        assert_eq!(loaded.predicate_label(&aime), Some("aime"));

        // Une seconde sauvegarde remplace le snapshot sans laisser de
        // fichier temporaire derrière elle.
        graph.save_to_file(&path).unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "fichiers temporaires orphelins: {leftovers:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
