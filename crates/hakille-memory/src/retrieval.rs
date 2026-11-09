//! Rappel hybride explicable : BM25 lexical + cosinus réel, fusion par rang.
//! Le cache est dérivé du graphe, jamais une deuxième source de vérité.

use crate::{ContextPartition, HnswVectorStore, KnowledgeGraph, NodeValue, Triplet, VectorIndex};
use hakille_core::{Result, TripletId, TrustLevel};
use std::collections::{BTreeMap, BTreeSet};

pub type FallibleEmbedder<'a> = dyn Fn(&str) -> Result<Vec<f32>> + 'a;

#[derive(Clone, Debug)]
pub struct RecallPolicy {
    pub partition: ContextPartition,
    pub limit: usize,
    /// Budget UTF-8 en octets, borne conservatrice indépendante du tokenizer.
    pub max_bytes: usize,
    pub min_cosine: f32,
    /// Indexation froide bornée par requête ; le reste reste lexicalement accessible.
    pub max_new_embeddings: usize,
}

impl Default for RecallPolicy {
    fn default() -> Self {
        Self {
            partition: ContextPartition::Personal,
            limit: 12,
            max_bytes: 6000,
            min_cosine: 0.40,
            max_new_embeddings: 32,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MemoryEvidence {
    pub id: TripletId,
    pub text: String,
    pub object: String,
    pub lexical_score: f64,
    pub cosine: Option<f32>,
    pub score: f64,
}

#[derive(Default, Debug)]
pub struct RecallReport {
    pub hits: Vec<MemoryEvidence>,
    pub semantic_used: bool,
    pub warning: Option<String>,
    pub omitted: usize,
}

struct Document {
    text: String,
    terms: BTreeMap<String, usize>,
    length: usize,
    embedding: Option<Vec<f32>>,
}

#[derive(Default)]
pub struct HybridRetriever {
    documents: BTreeMap<TripletId, Document>,
    vectors: HnswVectorStore,
}

/// Les espaces partagés sont lisibles ; sélectionner Shared n'ouvre pas les
/// partitions privées. Default contient les faits historiques non partitionnés.
pub fn visible(stored: &ContextPartition, active: &ContextPartition) -> bool {
    stored == active || matches!(stored, ContextPartition::Default | ContextPartition::Shared)
}

fn normalize(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'à' | 'â' | 'ä' => 'a',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

fn words(text: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "le", "la", "les", "un", "une", "des", "du", "de", "et", "est", "a", "au", "aux", "en",
        "dans", "pour", "que", "qui", "quel", "quelle", "quels", "quelles", "comment", "je", "tu",
        "il", "elle", "mon", "ma", "mes", "ton", "ta", "tes", "nous", "vous", "ce", "cette", "sur",
        "the", "is", "of", "and", "what",
    ];
    normalize(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && !STOP.contains(w))
        .map(str::to_owned)
        .collect()
}

pub fn lexical_terms(text: &str) -> BTreeSet<String> {
    words(text).into_iter().collect()
}

pub fn object_text(value: &NodeValue) -> String {
    match value {
        NodeValue::Text(s) => s.clone(),
        other => format!("{other:?}"),
    }
}

pub fn triplet_text(graph: &KnowledgeGraph, t: &Triplet) -> String {
    format!(
        "{} — {} — {}",
        graph.subject_label(&t.subject).unwrap_or("Sujet"),
        graph.predicate_label(&t.predicate).unwrap_or("Relation"),
        object_text(&t.object)
    )
}

fn valid_embedding(vector: &[f32]) -> bool {
    vector.len() == crate::EMBEDDING_DIM
        && vector.iter().all(|v| v.is_finite())
        && vector.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() > 0.0
}

impl HybridRetriever {
    /// Le propriétaire doit réinitialiser ce cache quand le modèle change.
    /// Les suppressions, changements de texte et de partition sont détectés
    /// à chaque recherche, même après une mutation directe du graphe public.
    pub fn search(
        &mut self,
        graph: &KnowledgeGraph,
        query: &str,
        policy: &RecallPolicy,
        embedder: Option<&FallibleEmbedder<'_>>,
    ) -> Result<RecallReport> {
        let mut report = RecallReport::default();
        if query.trim().is_empty() || policy.limit == 0 || policy.max_bytes == 0 {
            return Ok(report);
        }
        let eligible: BTreeMap<_, _> = graph
            .all_triplets()
            .into_iter()
            .filter(|t| {
                visible(&t.metadata.context, &policy.partition)
                    && matches!(
                        t.metadata.trust_level,
                        TrustLevel::User | TrustLevel::System
                    )
                    && t.metadata.validate().is_ok()
            })
            .map(|t| (t.id.clone(), t))
            .collect();
        let before = self.documents.len();
        self.documents.retain(|id, _| eligible.contains_key(id));
        let mut changed = before != self.documents.len();
        let mut attempted = 0usize;
        let mut deferred = 0usize;
        for (id, triplet) in &eligible {
            let text = triplet_text(graph, triplet);
            if self.documents.get(id).is_none_or(|doc| doc.text != text) {
                let mut terms = BTreeMap::new();
                let tokens = words(&text);
                for word in &tokens {
                    *terms.entry(word.clone()).or_insert(0) += 1;
                }
                self.documents.insert(
                    id.clone(),
                    Document {
                        text,
                        terms,
                        length: tokens.len(),
                        embedding: None,
                    },
                );
                changed = true;
            }
            let doc = self.documents.get_mut(id).expect("document indexé");
            if doc.embedding.is_none()
                && let Some(embedder) = embedder
            {
                if attempted >= policy.max_new_embeddings {
                    deferred += 1;
                    continue;
                }
                attempted += 1;
                match embedder(&doc.text) {
                    Ok(vector) if valid_embedding(&vector) => { doc.embedding = Some(vector); changed = true; }
                    Ok(_) => report.warning = Some("Embedding invalide : dimension, valeurs non finies ou norme nulle ; rappel lexical disponible.".into()),
                    Err(error) => report.warning = Some(format!("Encodeur indisponible : {error} ; rappel lexical disponible.")),
                }
            }
        }
        if deferred > 0 {
            let backlog = format!(
                "{deferred} souvenirs restent à indexer sémantiquement ; rappel lexical complet disponible."
            );
            report.warning = Some(match report.warning {
                Some(error) => format!("{error} {backlog}"),
                None => backlog,
            });
        }
        if changed {
            self.vectors = HnswVectorStore::from_embeddings(
                self.documents
                    .iter()
                    .filter_map(|(id, doc)| doc.embedding.clone().map(|v| (id.clone(), v)))
                    .collect(),
            )?;
        }

        let terms = lexical_terms(query);
        let count = self.documents.len() as f64;
        let avg = (self.documents.values().map(|d| d.length).sum::<usize>() as f64
            / count.max(1.0))
        .max(1.0);
        let frequencies: BTreeMap<_, _> = terms
            .iter()
            .map(|term| {
                (
                    term,
                    self.documents
                        .values()
                        .filter(|d| d.terms.contains_key(term))
                        .count() as f64,
                )
            })
            .collect();
        let mut lexical: Vec<(TripletId, f64)> = self
            .documents
            .iter()
            .filter_map(|(id, doc)| {
                let mut score = 0.0;
                for term in &terms {
                    let tf = *doc.terms.get(term).unwrap_or(&0) as f64;
                    if tf == 0.0 {
                        continue;
                    }
                    let df = frequencies[term];
                    let idf = (1.0 + (count - df + 0.5) / (df + 0.5)).ln();
                    score += idf * tf * 2.2 / (tf + 1.2 * (0.25 + 0.75 * doc.length as f64 / avg));
                }
                (score > 0.0).then_some((id.clone(), score))
            })
            .collect();
        lexical.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let mut fused: BTreeMap<TripletId, (f64, f64, Option<f32>)> = BTreeMap::new();
        for (rank, (id, score)) in lexical.iter().enumerate() {
            fused.insert(id.clone(), (1.0 / (60 + rank + 1) as f64, *score, None));
        }
        if let Some(embedder) = embedder
            && !self.vectors.is_empty()
        {
            match embedder(query) {
                Ok(vector) if valid_embedding(&vector) => {
                    report.semantic_used = true;
                    for (rank, (id, cosine)) in self
                        .vectors
                        .search(&vector, policy.limit.saturating_mul(4).max(32))?
                        .into_iter()
                        .filter(|(_, score)| *score >= policy.min_cosine)
                        .enumerate()
                    {
                        let entry = fused.entry(id).or_insert((0.0, 0.0, None));
                        entry.0 += 1.0 / (60 + rank + 1) as f64;
                        entry.2 = Some(cosine);
                    }
                }
                Ok(_) => {
                    report.warning =
                        Some("Embedding de requête invalide ; rappel lexical disponible.".into())
                }
                Err(error) => {
                    report.warning = Some(format!(
                        "Embedding de requête : {error} ; rappel lexical disponible."
                    ))
                }
            }
        }
        let mut ranked: Vec<_> = fused.into_iter().collect();
        ranked.sort_by(|a, b| {
            b.1.0
                .total_cmp(&a.1.0)
                .then_with(|| {
                    eligible[&b.0]
                        .metadata
                        .confidence
                        .total_cmp(&eligible[&a.0].metadata.confidence)
                })
                .then_with(|| {
                    eligible[&b.0]
                        .metadata
                        .weight
                        .total_cmp(&eligible[&a.0].metadata.weight)
                })
                .then_with(|| a.0.cmp(&b.0))
        });
        let total = ranked.len();
        let mut bytes = 0usize;
        for (id, (score, lexical_score, cosine)) in ranked {
            let t = eligible[&id];
            let text = format!(
                "[fait {} ; source={:?} ; confiance={:.2}] {}",
                id.0, t.metadata.source, t.metadata.confidence, self.documents[&id].text
            );
            if report.hits.len() == policy.limit
                || bytes.saturating_add(text.len()) > policy.max_bytes
            {
                continue;
            }
            bytes += text.len();
            report.hits.push(MemoryEvidence {
                id,
                text,
                object: object_text(&t.object),
                lexical_score,
                cosine,
                score,
            });
        }
        report.omitted = total - report.hits.len();
        Ok(report)
    }
}
