use crate::context_partition::ContextPartition;
use crate::graph::KnowledgeGraph;
use crate::triplet::{NodeValue, Triplet, TripletSource};
use hakille_core::{NodeId, PredicateId, Timestamp, TrustLevel};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Filter criteria for object values in a declarative graph query (§31b).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObjectFilter {
    Any,
    ExactEntity(NodeId),
    ContainsText(String),
    NumericRange { min: f64, max: f64 },
    Boolean(bool),
}

/// Declarative Graph Query specification (§31b).
/// Enables typed traversal and filtering without ad-hoc manual graph loops.
///
/// Honnêteté documentée (périmètre) : `execute` travaille en O(N) RAM —
/// scan linéaire de `graph.all_triplets()` + tri en mémoire, SANS push-down
/// vers redb (hors périmètre Agent C : le DSL filtre en RAM, il ne génère
/// pas de plan d'exécution storage). Viable pour un graphe local ; si N
/// explose, le push-down redb se fera derrière cette même API `execute`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQuery {
    pub subject: Option<NodeId>,
    pub predicate: Option<PredicateId>,
    pub object_filter: ObjectFilter,
    pub min_weight: f64,
    pub min_confidence: f64,
    pub partition: Option<ContextPartition>,
    /// Filtre source (None = toutes sources). `#[serde(default)]` : les
    /// requêtes sérialisées avant ce champ restent désérialisables.
    #[serde(default)]
    pub source: Option<TripletSource>,
    /// Filtre niveau de confiance (None = tous niveaux). `#[serde(default)]`
    /// pour la même compatibilité de désérialisation.
    #[serde(default)]
    pub trust: Option<TrustLevel>,
    /// Fenêtre de création inclusive `[start, end]` (None = sans borne).
    /// `start > end` ne peut rien matcher ⇒ filtre vide par construction
    /// (documenté, pas d'erreur : même convention que `NumericRange`).
    #[serde(default)]
    pub created_range: Option<(Timestamp, Timestamp)>,
    pub limit: usize,
    /// Pagination : nombre de résultats à sauter APRÈS le tri déterministe
    /// et AVANT `limit`. `offset > N` ⇒ `[]` (pas de panique).
    #[serde(default)]
    pub offset: usize,
}

impl Default for GraphQuery {
    fn default() -> Self {
        Self {
            subject: None,
            predicate: None,
            object_filter: ObjectFilter::Any,
            min_weight: 0.0,
            min_confidence: 0.0,
            partition: None,
            source: None,
            trust: None,
            created_range: None,
            limit: 100,
            offset: 0,
        }
    }
}

impl GraphQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_subject(mut self, subject: NodeId) -> Self {
        self.subject = Some(subject);
        self
    }

    pub fn with_predicate(mut self, predicate: PredicateId) -> Self {
        self.predicate = Some(predicate);
        self
    }

    pub fn with_text_filter(mut self, substring: impl Into<String>) -> Self {
        self.object_filter = ObjectFilter::ContainsText(substring.into());
        self
    }

    /// Filtre entité exacte sur l'objet.
    pub fn with_object_exact_entity(mut self, entity: NodeId) -> Self {
        self.object_filter = ObjectFilter::ExactEntity(entity);
        self
    }

    /// Filtre plage numérique inclusive sur l'objet.
    ///
    /// Validation documentée : si `min > max`, aucun `Number` ne peut
    /// satisfaire `n >= min && n <= max` ⇒ la requête matche RIEN (filtre
    /// vide par construction, pas d'erreur). Les appelants qui veulent un
    /// `Err` explicite doivent valider `min <= max` avant d'appeler.
    pub fn with_object_numeric_range(mut self, min: f64, max: f64) -> Self {
        self.object_filter = ObjectFilter::NumericRange { min, max };
        self
    }

    /// Filtre booléen sur l'objet.
    pub fn with_object_boolean(mut self, value: bool) -> Self {
        self.object_filter = ObjectFilter::Boolean(value);
        self
    }

    /// Filtre source du triplet (User / System / Inferred / External).
    pub fn with_source(mut self, source: TripletSource) -> Self {
        self.source = Some(source);
        self
    }

    /// Filtre niveau de confiance du triplet.
    pub fn with_trust(mut self, trust: TrustLevel) -> Self {
        self.trust = Some(trust);
        self
    }

    /// Fenêtre de création inclusive `[start, end]`.
    /// `start > end` ⇒ filtre vide par construction (documenté, cf.
    /// `with_object_numeric_range`).
    pub fn with_created_range(mut self, start: Timestamp, end: Timestamp) -> Self {
        self.created_range = Some((start, end));
        self
    }

    pub fn with_min_weight(mut self, min_weight: f64) -> Self {
        self.min_weight = min_weight;
        self
    }

    pub fn with_min_confidence(mut self, min_confidence: f64) -> Self {
        self.min_confidence = min_confidence;
        self
    }

    pub fn with_partition(mut self, partition: ContextPartition) -> Self {
        self.partition = Some(partition);
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Pagination : sauter `offset` résultats après tri, avant `limit`.
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    fn matches(&self, triplet: &Triplet) -> bool {
        // Subject filter
        if let Some(ref subj) = self.subject
            && triplet.subject != *subj
        {
            return false;
        }

        // Predicate filter
        if let Some(ref pred) = self.predicate
            && triplet.predicate != *pred
        {
            return false;
        }

        // Object filter
        match &self.object_filter {
            ObjectFilter::Any => {}
            ObjectFilter::ExactEntity(id) => match &triplet.object {
                NodeValue::Entity(e) if e == id => {}
                _ => return false,
            },
            ObjectFilter::ContainsText(query) => match &triplet.object {
                NodeValue::Text(s) if s.to_lowercase().contains(&query.to_lowercase()) => {}
                _ => return false,
            },
            ObjectFilter::NumericRange { min, max } => match &triplet.object {
                // Note : si min > max, cette garde est structurellement
                // fausse ⇒ filtre vide (convention documentée).
                NodeValue::Number(n) if n >= min && n <= max => {}
                _ => return false,
            },
            ObjectFilter::Boolean(expected) => match &triplet.object {
                NodeValue::Boolean(b) if b == expected => {}
                _ => return false,
            },
        }

        // Weight filter (temporal decay)
        if triplet.metadata.weight < self.min_weight {
            return false;
        }

        // Confidence filter
        if triplet.metadata.confidence < self.min_confidence {
            return false;
        }

        // Partition filter
        if let Some(ref part) = self.partition
            && triplet.metadata.context != *part
        {
            return false;
        }

        // Source filter
        if let Some(ref src) = self.source
            && triplet.metadata.source != *src
        {
            return false;
        }

        // Trust filter
        if let Some(ref trust) = self.trust
            && triplet.metadata.trust_level != *trust
        {
            return false;
        }

        // Created-range filter (bornes inclusives).
        if let Some((start, end)) = &self.created_range
            && (triplet.metadata.created_at < *start || triplet.metadata.created_at > *end)
        {
            return false;
        }

        true
    }

    /// Execute declarative query against the knowledge graph.
    ///
    /// Déterminisme garanti : TOUS les triplets matchés sont collectés puis
    /// triés par poids desc, puis confiance desc (puis `TripletId` croissant
    /// en ultime départage de stabilité), AVANT d'appliquer `offset`+`limit`.
    /// Deux appels identiques sur le même graphe rendent le même ordre —
    /// l'ancien `break` précoce dépendait de l'ordre d'itération et rendait
    /// un préfixe arbitraire.
    ///
    /// - `limit == 0` ⇒ `[]` (early return, bug `with_limit(0)` corrigé :
    ///   l'ancienne boucle poussait 1 élément avant de couper).
    /// - `offset >= N` ⇒ `[]` (pas de panique, saturation documentée).
    /// - Complexité : O(N) RAM + tri O(M log M) — pas de push-down redb
    ///   (hors périmètre, voir doc du struct).
    pub fn execute<'a>(&self, graph: &'a KnowledgeGraph) -> Vec<&'a Triplet> {
        if self.limit == 0 {
            return Vec::new();
        }
        let mut results: Vec<&'a Triplet> = graph
            .all_triplets()
            .into_iter()
            .filter(|t| self.matches(t))
            .collect();
        results.sort_by(|a, b| {
            b.metadata
                .weight
                .partial_cmp(&a.metadata.weight)
                .unwrap_or(Ordering::Equal)
                .then(
                    b.metadata
                        .confidence
                        .partial_cmp(&a.metadata.confidence)
                        .unwrap_or(Ordering::Equal),
                )
                .then_with(|| a.id.cmp(&b.id))
        });
        let start = self.offset.min(results.len());
        let end = start.saturating_add(self.limit).min(results.len());
        results[start..end].to_vec()
    }

    /// Pagination explicite : même tri déterministe que `execute`, en
    /// surchargeant `offset`/`limit` pour la page demandée (sans muter
    /// `self`). `limit == 0` ⇒ `[]`.
    pub fn execute_paged<'a>(
        &self,
        graph: &'a KnowledgeGraph,
        offset: usize,
        limit: usize,
    ) -> Vec<&'a Triplet> {
        if limit == 0 {
            return Vec::new();
        }
        let mut paged = self.clone();
        paged.offset = offset;
        paged.limit = limit;
        paged.execute(graph)
    }
}

/// Query Builder DSL for fluent syntax (§31b).
pub struct GraphQueryBuilder {
    inner: GraphQuery,
}

impl GraphQueryBuilder {
    pub fn select() -> Self {
        Self {
            inner: GraphQuery::new(),
        }
    }

    pub fn where_subject(mut self, subject: NodeId) -> Self {
        self.inner.subject = Some(subject);
        self
    }

    pub fn where_predicate(mut self, predicate: PredicateId) -> Self {
        self.inner.predicate = Some(predicate);
        self
    }

    pub fn where_object_contains(mut self, text: impl Into<String>) -> Self {
        self.inner.object_filter = ObjectFilter::ContainsText(text.into());
        self
    }

    /// Filtre entité exacte sur l'objet.
    pub fn where_object_exact_entity(mut self, entity: NodeId) -> Self {
        self.inner.object_filter = ObjectFilter::ExactEntity(entity);
        self
    }

    /// Filtre plage numérique inclusive. `min > max` ⇒ filtre vide par
    /// construction (documenté sur `GraphQuery::with_object_numeric_range`).
    pub fn where_object_numeric_range(mut self, min: f64, max: f64) -> Self {
        self.inner.object_filter = ObjectFilter::NumericRange { min, max };
        self
    }

    /// Filtre booléen sur l'objet.
    pub fn where_object_boolean(mut self, value: bool) -> Self {
        self.inner.object_filter = ObjectFilter::Boolean(value);
        self
    }

    /// Filtre source du triplet.
    pub fn where_source(mut self, source: TripletSource) -> Self {
        self.inner.source = Some(source);
        self
    }

    /// Filtre niveau de confiance du triplet.
    pub fn where_trust(mut self, trust: TrustLevel) -> Self {
        self.inner.trust = Some(trust);
        self
    }

    /// Fenêtre de création inclusive `[start, end]`
    /// (`start > end` ⇒ filtre vide, documenté).
    pub fn where_created_range(mut self, start: Timestamp, end: Timestamp) -> Self {
        self.inner.created_range = Some((start, end));
        self
    }

    pub fn where_weight_above(mut self, weight: f64) -> Self {
        self.inner.min_weight = weight;
        self
    }

    pub fn where_confidence_above(mut self, confidence: f64) -> Self {
        self.inner.min_confidence = confidence;
        self
    }

    pub fn where_partition(mut self, partition: ContextPartition) -> Self {
        self.inner.partition = Some(partition);
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.inner.limit = limit;
        self
    }

    /// Pagination : résultats à sauter après tri, avant `limit`.
    pub fn offset(mut self, offset: usize) -> Self {
        self.inner.offset = offset;
        self
    }

    pub fn build(self) -> GraphQuery {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triplet::{TripletMetadata, TripletSource};
    use chrono::{Duration, Utc};
    use hakille_core::{NodeId, PredicateId, Timestamp, TripletId, TrustLevel};

    fn metadata(
        weight: f64,
        confidence: f64,
        source: TripletSource,
        trust: TrustLevel,
        created_at: Timestamp,
    ) -> TripletMetadata {
        TripletMetadata {
            weight,
            created_at: created_at.clone(),
            last_activated: created_at,
            source,
            confidence,
            context: ContextPartition::Default,
            trust_level: trust,
            decay_rate: 0.0,
        }
    }

    fn text_triplet(subject: &NodeId, object: &str, weight: f64, confidence: f64) -> Triplet {
        Triplet {
            id: TripletId(uuid::Uuid::new_v4()),
            subject: subject.clone(),
            // Prédicat frais par triplet : le routage contradiction (§8)
            // refuse même S/P + objets différents — voir test `lib.rs`.
            predicate: PredicateId(uuid::Uuid::new_v4()),
            object: NodeValue::Text(object.to_string()),
            metadata: metadata(
                weight,
                confidence,
                TripletSource::User,
                TrustLevel::User,
                Timestamp::now(),
            ),
            node_type: None,
        }
    }

    #[test]
    fn test_limit_zero_returns_empty() {
        let mut graph = KnowledgeGraph::new();
        let subj = NodeId(uuid::Uuid::new_v4());
        graph.insert(text_triplet(&subj, "Rust", 0.9, 0.9));
        // with_limit(0) ⇒ [] (bug corrigé : early return, pas 1 élément).
        let q = GraphQuery::new().with_limit(0);
        assert!(q.execute(&graph).is_empty());
        // Builder pareil, et execute_paged avec limit 0 aussi.
        let qb = GraphQueryBuilder::select().limit(0).build();
        assert!(qb.execute(&graph).is_empty());
        assert!(GraphQuery::new().execute_paged(&graph, 0, 0).is_empty());
    }

    #[test]
    fn test_execute_sorts_weight_then_confidence_deterministically() {
        let mut graph = KnowledgeGraph::new();
        let subj = NodeId(uuid::Uuid::new_v4());
        // Même sujet, prédicats distincts (pas de conflit), même filtre texte.
        let mut t_low = text_triplet(&subj, "Rust low", 0.5, 0.99);
        let mut t_high_conf = text_triplet(&subj, "Rust mid", 0.9, 0.60);
        let mut t_top = text_triplet(&subj, "Rust top", 0.9, 0.95);
        // IDs fixés pour un ordre attendu lisible (départage ultime).
        t_low.id = TripletId(uuid::Uuid::from_u128(3));
        t_high_conf.id = TripletId(uuid::Uuid::from_u128(2));
        t_top.id = TripletId(uuid::Uuid::from_u128(1));
        graph.insert(t_low);
        graph.insert(t_high_conf);
        graph.insert(t_top);

        let q = GraphQuery::new().with_text_filter("Rust");
        let results = q.execute(&graph);
        assert_eq!(results.len(), 3);
        // Poids desc d'abord : les deux 0.9 devant le 0.5.
        assert!((results[0].metadata.weight - 0.9).abs() < 1e-9);
        assert!((results[1].metadata.weight - 0.9).abs() < 1e-9);
        assert!((results[2].metadata.weight - 0.5).abs() < 1e-9);
        // Confiance desc ensuite : 0.95 avant 0.60.
        assert!(results[0].metadata.confidence > results[1].metadata.confidence);
        // Stabilité : second appel ⇒ même ordre.
        let again = q.execute(&graph);
        assert_eq!(
            results.iter().map(|t| &t.id).collect::<Vec<_>>(),
            again.iter().map(|t| &t.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_builder_full_filters_and_offset_pagination() {
        let mut graph = KnowledgeGraph::new();
        let subj = NodeId(uuid::Uuid::new_v4());
        for i in 0..5 {
            graph.insert(text_triplet(
                &subj,
                &format!("Rust item {i}"),
                0.9 - i as f64 * 0.1,
                0.9,
            ));
        }

        // Builder complet : tous les filtres chaînés compilent et s'appliquent.
        let page1 = GraphQueryBuilder::select()
            .where_subject(subj.clone())
            .where_object_contains("Rust")
            .where_weight_above(0.0)
            .where_confidence_above(0.0)
            .where_partition(ContextPartition::Default)
            .where_source(TripletSource::User)
            .where_trust(TrustLevel::User)
            .limit(2)
            .offset(0)
            .build();
        let r1 = page1.execute(&graph);
        assert_eq!(r1.len(), 2);

        // Page 2 : offset décale APRÈS le tri (poids desc ⇒ items suivants).
        let page2 = GraphQueryBuilder::select()
            .where_subject(subj.clone())
            .where_object_contains("Rust")
            .limit(2)
            .offset(2)
            .build();
        let r2 = page2.execute(&graph);
        assert_eq!(r2.len(), 2);
        assert!(r2[0].metadata.weight < r1[0].metadata.weight);

        // execute_paged équivalent à offset+limit dans execute.
        let base = GraphQueryBuilder::select()
            .where_subject(subj.clone())
            .where_object_contains("Rust")
            .build();
        let via_paged = base.execute_paged(&graph, 2, 2);
        assert_eq!(
            via_paged.iter().map(|t| &t.id).collect::<Vec<_>>(),
            r2.iter().map(|t| &t.id).collect::<Vec<_>>()
        );

        // Offset au-delà de N ⇒ [] sans panique.
        assert!(base.execute_paged(&graph, 100, 10).is_empty());
        let beyond = GraphQuery::new().with_limit(10).with_offset(100);
        assert!(beyond.execute(&graph).is_empty());
    }

    #[test]
    fn test_object_entity_numeric_boolean_source_trust_created_filters() {
        let mut graph = KnowledgeGraph::new();
        let subj = NodeId(uuid::Uuid::new_v4());
        let entity = NodeId(uuid::Uuid::new_v4());
        let now = Utc::now();
        let old_ts = Timestamp(now - Duration::days(10));
        let recent_ts = Timestamp(now);

        let mk =
            |object: NodeValue, source: TripletSource, trust: TrustLevel, created_at: Timestamp| {
                Triplet {
                    id: TripletId(uuid::Uuid::new_v4()),
                    subject: subj.clone(),
                    predicate: PredicateId(uuid::Uuid::new_v4()),
                    object,
                    metadata: metadata(0.9, 0.9, source, trust, created_at),
                    node_type: None,
                }
            };
        graph.insert(mk(
            NodeValue::Entity(entity.clone()),
            TripletSource::System,
            TrustLevel::System,
            old_ts.clone(),
        ));
        graph.insert(mk(
            NodeValue::Number(42.0),
            TripletSource::User,
            TrustLevel::User,
            recent_ts.clone(),
        ));
        graph.insert(mk(
            NodeValue::Boolean(true),
            TripletSource::Inferred,
            TrustLevel::Guest,
            recent_ts.clone(),
        ));

        // Entité exacte.
        let r = GraphQueryBuilder::select()
            .where_object_exact_entity(entity.clone())
            .build()
            .execute(&graph);
        assert_eq!(r.len(), 1);

        // Plage numérique : [40, 50] matche, [0, 10] non.
        let r = GraphQueryBuilder::select()
            .where_object_numeric_range(40.0, 50.0)
            .build()
            .execute(&graph);
        assert_eq!(r.len(), 1);
        let r = GraphQueryBuilder::select()
            .where_object_numeric_range(0.0, 10.0)
            .build()
            .execute(&graph);
        assert!(r.is_empty());
        // Plage inversée min > max ⇒ filtre vide documenté (pas d'Err).
        let r = GraphQuery::new()
            .with_object_numeric_range(50.0, 40.0)
            .execute(&graph);
        assert!(r.is_empty());

        // Booléen.
        let r = GraphQueryBuilder::select()
            .where_object_boolean(true)
            .build()
            .execute(&graph);
        assert_eq!(r.len(), 1);
        let r = GraphQuery::new().with_object_boolean(false).execute(&graph);
        assert!(r.is_empty());

        // Source + trust.
        let r = GraphQueryBuilder::select()
            .where_source(TripletSource::System)
            .build()
            .execute(&graph);
        assert_eq!(r.len(), 1);
        let r = GraphQueryBuilder::select()
            .where_trust(TrustLevel::Guest)
            .build()
            .execute(&graph);
        assert_eq!(r.len(), 1);
        // Combinaison source × trust disjointe ⇒ vide.
        let r = GraphQuery::new()
            .with_source(TripletSource::System)
            .with_trust(TrustLevel::Guest)
            .execute(&graph);
        assert!(r.is_empty());

        // Fenêtre de création : large ⇒ 3, récente (borne basse = now - 1h)
        // ⇒ exclut le vieux triplet système.
        let wide = GraphQuery::new()
            .with_created_range(
                Timestamp(now - Duration::days(30)),
                Timestamp(now + Duration::days(1)),
            )
            .execute(&graph);
        assert_eq!(wide.len(), 3);
        let recent_only = GraphQueryBuilder::select()
            .where_created_range(
                Timestamp(now - Duration::hours(1)),
                Timestamp(now + Duration::days(1)),
            )
            .build()
            .execute(&graph);
        assert_eq!(recent_only.len(), 2);
        // Fenêtre inversée ⇒ vide documenté.
        let inverted = GraphQuery::new()
            .with_created_range(
                Timestamp(now + Duration::days(1)),
                Timestamp(now - Duration::days(1)),
            )
            .execute(&graph);
        assert!(inverted.is_empty());
    }
}
