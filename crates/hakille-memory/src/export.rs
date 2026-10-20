use crate::graph::KnowledgeGraph;
use crate::triplet::Triplet;
use hakille_core::{NodeId, PredicateId, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Version du format d'export (chaîne `"1"` — comparée strictement à l'import).
pub const EXPORT_VERSION: &str = "1";
/// Marqueur de format attendu dans `GraphExport.format`.
pub const EXPORT_FORMAT_TAG: &str = "hakille-graph-export";
/// Générateur renseigné dans `ExportMetadata.generator`.
pub const EXPORT_GENERATOR: &str = "hakille-memory 0.1.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    Json,
    /// Conservé pour compatibilité d'API mais non supporté : aucun backend
    /// CBOR dans `hakille-memory` par choix (voir [`ExportError::CborNotSupported`]).
    /// Toute tentative rend `Err(CborNotSupported)` explicite (test attesté).
    Cbor,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ExportError {
    #[error(
        "CBOR export is not supported (no CBOR dependency in hakille-memory by design); use ExportFormat::Json"
    )]
    CborNotSupported,
    #[error("JSON serialization failed: {0}")]
    Serialization(String),
    #[error("invalid export JSON: {0}")]
    InvalidJson(String),
    #[error("unsupported export version: expected \"{expected}\", got \"{got}\"")]
    UnsupportedVersion { expected: String, got: String },
    #[error("unknown export format: expected \"{expected}\", got \"{got}\"")]
    UnknownFormat { expected: String, got: String },
    #[error("export count mismatch: metadata.count={expected}, triplets.len()={got}")]
    CountMismatch { expected: usize, got: usize },
}

impl From<ExportError> for hakille_core::HakilleError {
    fn from(e: ExportError) -> Self {
        hakille_core::HakilleError::SerializationError(e.to_string())
    }
}

fn default_metadata_version() -> u32 {
    1
}

fn default_metadata_exported_at() -> Timestamp {
    Timestamp::now()
}

fn default_generator() -> String {
    EXPORT_GENERATOR.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportMetadata {
    /// Version numérique du format (exigée par la spec IMP-02 : 1).
    #[serde(default = "default_metadata_version")]
    pub version: u32,
    /// Horodatage miroir du top-level `exported_at` (spec IMP-02).
    #[serde(default = "default_metadata_exported_at")]
    pub exported_at: Timestamp,
    pub count: usize,
    #[serde(default = "default_generator")]
    pub generator: String,
}

/// Libellés résolus via le registre du graphe (`subject_label` /
/// `predicate_label`) au moment de l'export.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphLabels {
    #[serde(default)]
    pub subjects: BTreeMap<NodeId, String>,
    #[serde(default)]
    pub predicates: BTreeMap<PredicateId, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphExport {
    pub format: String,
    pub version: String,
    pub exported_at: Timestamp,
    pub triplets: Vec<Triplet>,
    #[serde(default)]
    pub labels: GraphLabels,
    pub metadata: ExportMetadata,
}

/// Bilan d'un [`import_into_graph`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportReport {
    /// Triplets effectivement insérés (`Inserted` + `ContradictionResolved`).
    pub inserted: usize,
    /// Triplets refusés en attente d'arbitrage (`NeedsUserResolution`).
    pub refused: usize,
}

/// Exporte le graphe en JSON pretty (`serde_json::to_vec_pretty`).
///
/// - `Json` : sérialise `GraphExport { format: "hakille-graph-export",
///   version: "1", exported_at, triplets, labels, metadata }`.
/// - `Cbor` : `Err(ExportError::CborNotSupported)` explicite — pas de
///   dépendance CBOR dans `hakille-memory` (choix assumé, documenté ici ;
///   le variant est conservé pour ne pas casser l'API).
pub fn export_graph(graph: &KnowledgeGraph, format: ExportFormat) -> Result<Vec<u8>, ExportError> {
    match format {
        ExportFormat::Cbor => Err(ExportError::CborNotSupported),
        ExportFormat::Json => {
            let triplets: Vec<Triplet> = graph.all_triplets().into_iter().cloned().collect();
            let mut subjects: BTreeMap<NodeId, String> = BTreeMap::new();
            let mut predicates: BTreeMap<PredicateId, String> = BTreeMap::new();
            for t in &triplets {
                if let Some(label) = graph.subject_label(&t.subject) {
                    subjects
                        .entry(t.subject.clone())
                        .or_insert_with(|| label.to_string());
                }
                if let Some(label) = graph.predicate_label(&t.predicate) {
                    predicates
                        .entry(t.predicate.clone())
                        .or_insert_with(|| label.to_string());
                }
            }
            let exported_at = Timestamp::now();
            let payload = GraphExport {
                format: EXPORT_FORMAT_TAG.to_string(),
                version: EXPORT_VERSION.to_string(),
                exported_at: exported_at.clone(),
                metadata: ExportMetadata {
                    version: 1,
                    exported_at,
                    count: triplets.len(),
                    generator: EXPORT_GENERATOR.to_string(),
                },
                labels: GraphLabels {
                    subjects,
                    predicates,
                },
                triplets,
            };
            serde_json::to_vec_pretty(&payload)
                .map_err(|e| ExportError::Serialization(e.to_string()))
        }
    }
}

/// Alias historique rendant `hakille_core::Result` (compatibilité avec le
/// code ayant utilisé `try_export_graph(graph, Json)`).
pub fn try_export_graph(
    graph: &KnowledgeGraph,
    format: ExportFormat,
) -> hakille_core::Result<Vec<u8>> {
    export_graph(graph, format).map_err(hakille_core::HakilleError::from)
}

/// Importe des octets d'export JSON et rend le `GraphExport` validé.
///
/// Refus explicites :
/// - `version != "1"` (`UnsupportedVersion`) ;
/// - `metadata.version != 1` (version numérique unifiée, même refus) ;
/// - `format` inconnu (`UnknownFormat`, attendu `"hakille-graph-export"`) ;
/// - `metadata.count != triplets.len()` (`CountMismatch`).
pub fn import_graph(bytes: &[u8]) -> Result<GraphExport, ExportError> {
    let export: GraphExport =
        serde_json::from_slice(bytes).map_err(|e| ExportError::InvalidJson(e.to_string()))?;
    if export.version != EXPORT_VERSION {
        return Err(ExportError::UnsupportedVersion {
            expected: EXPORT_VERSION.to_string(),
            got: export.version,
        });
    }
    if export.metadata.version != 1 {
        return Err(ExportError::UnsupportedVersion {
            expected: "1".to_string(),
            got: export.metadata.version.to_string(),
        });
    }
    if export.format != EXPORT_FORMAT_TAG {
        return Err(ExportError::UnknownFormat {
            expected: EXPORT_FORMAT_TAG.to_string(),
            got: export.format,
        });
    }
    if export.metadata.count != export.triplets.len() {
        return Err(ExportError::CountMismatch {
            expected: export.metadata.count,
            got: export.triplets.len(),
        });
    }
    Ok(export)
}

/// Variante spec IMP-02 : rend directement les triplets contenus
/// (`import_graph(bytes) -> Vec<Triplet>`).
pub fn import_graph_triplets(bytes: &[u8]) -> Result<Vec<Triplet>, ExportError> {
    import_graph(bytes).map(|e| e.triplets)
}

/// Importe un export dans un graphe existant : boucle `graph.insert` avec
/// comptage. `Inserted` et `ContradictionResolved` comptent comme `inserted` ;
/// `NeedsUserResolution` (jamais d'écrasement silencieux) compte comme
/// `refused` sans insertion.
pub fn import_into_graph(
    graph: &mut KnowledgeGraph,
    bytes: &[u8],
) -> Result<ImportReport, ExportError> {
    let export = import_graph(bytes)?;
    let mut inserted = 0usize;
    let mut refused = 0usize;
    for triplet in export.triplets {
        match graph.insert(triplet) {
            crate::triplet::InsertResult::Inserted(_)
            | crate::triplet::InsertResult::ContradictionResolved { .. } => {
                inserted += 1;
            }
            crate::triplet::InsertResult::NeedsUserResolution { .. } => {
                refused += 1;
            }
        }
    }
    Ok(ImportReport { inserted, refused })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ContextPartition;
    use crate::triplet::{NodeValue, TripletMetadata, TripletSource};
    use hakille_core::{NodeId, PredicateId, TripletId, TrustLevel};

    fn sample_triplet_with_ids(subject: NodeId, predicate: PredicateId, text: &str) -> Triplet {
        Triplet {
            id: TripletId(uuid::Uuid::now_v7()),
            subject,
            predicate,
            object: NodeValue::Text(text.to_string()),
            metadata: TripletMetadata {
                weight: 0.9,
                created_at: Timestamp::now(),
                last_activated: Timestamp::now(),
                source: TripletSource::User,
                confidence: 0.99,
                context: ContextPartition::Default,
                trust_level: TrustLevel::User,
                decay_rate: 0.01,
            },
            node_type: None,
        }
    }

    #[test]
    fn test_export_import_roundtrip_json() {
        let mut graph = KnowledgeGraph::new();
        // Sujets / prédicats résolus via le registre (labels exportés).
        let alice = graph.resolve_subject("Alice");
        let bob = graph.resolve_subject("Bob");
        let aime = graph.resolve_predicate("aime");
        let boit = graph.resolve_predicate("boit");

        graph.insert(sample_triplet_with_ids(
            alice.clone(),
            aime.clone(),
            "Programmation en Rust",
        ));
        graph.insert(sample_triplet_with_ids(bob.clone(), boit.clone(), "Café"));

        let bytes = export_graph(&graph, ExportFormat::Json).unwrap();
        // Schéma JSON attendu : format, version "1", exported_at, triplets,
        // labels, metadata{count, generator}.
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["format"], "hakille-graph-export");
        assert_eq!(v["version"], "1");
        assert!(v.get("exported_at").is_some());
        assert_eq!(v["triplets"].as_array().unwrap().len(), 2);
        assert_eq!(v["metadata"]["count"], 2);
        assert_eq!(v["metadata"]["generator"], "hakille-memory 0.1.0");

        let back = import_graph(&bytes).unwrap();
        assert_eq!(back.format, "hakille-graph-export");
        assert_eq!(back.version, "1");
        assert_eq!(back.metadata.count, 2);
        assert_eq!(back.metadata.generator, "hakille-memory 0.1.0");
        assert_eq!(back.triplets.len(), 2);

        // Tous les triplets comparés (ensemble d'IDs identique).
        let mut before: Vec<TripletId> =
            graph.all_triplets().iter().map(|t| t.id.clone()).collect();
        let mut after: Vec<TripletId> = back.triplets.iter().map(|t| t.id.clone()).collect();
        before.sort();
        after.sort();
        assert_eq!(before, after);

        // Labels résolus via lookup et présents dans l'export.
        assert_eq!(
            back.labels.subjects.get(&alice).map(|s| s.as_str()),
            Some("Alice")
        );
        assert_eq!(
            back.labels.subjects.get(&bob).map(|s| s.as_str()),
            Some("Bob")
        );
        assert_eq!(
            back.labels.predicates.get(&aime).map(|s| s.as_str()),
            Some("aime")
        );
        assert_eq!(
            back.labels.predicates.get(&boit).map(|s| s.as_str()),
            Some("boit")
        );
    }

    #[test]
    fn test_export_empty_graph() {
        let graph = KnowledgeGraph::new();
        let bytes = export_graph(&graph, ExportFormat::Json).unwrap();
        let export = import_graph(&bytes).unwrap();
        assert_eq!(export.version, "1");
        assert!(export.triplets.is_empty());
        assert_eq!(export.metadata.count, 0);
    }

    #[test]
    fn test_import_rejects_unknown_version() {
        let graph = KnowledgeGraph::new();
        let bytes = export_graph(&graph, ExportFormat::Json).unwrap();
        let mut v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        v["version"] = serde_json::Value::String("2".to_string());
        let bad = serde_json::to_vec(&v).unwrap();
        let err = import_graph(&bad).unwrap_err();
        assert!(
            matches!(err, ExportError::UnsupportedVersion { .. }),
            "attendu UnsupportedVersion, obtenu: {err}"
        );
    }

    #[test]
    fn test_import_rejects_bad_metadata_version() {
        let graph = KnowledgeGraph::new();
        let bytes = export_graph(&graph, ExportFormat::Json).unwrap();
        let mut v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        v["metadata"]["version"] = serde_json::Value::Number(2.into());
        let bad = serde_json::to_vec(&v).unwrap();
        let err = import_graph(&bad).unwrap_err();
        assert!(
            matches!(err, ExportError::UnsupportedVersion { .. }),
            "metadata.version != 1 doit être refusé, obtenu: {err}"
        );
    }

    #[test]
    fn test_import_rejects_count_mismatch() {
        let graph = KnowledgeGraph::new();
        let bytes = export_graph(&graph, ExportFormat::Json).unwrap();
        let mut v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        v["metadata"]["count"] = serde_json::Value::Number(42.into());
        let bad = serde_json::to_vec(&v).unwrap();
        let err = import_graph(&bad).unwrap_err();
        assert!(
            matches!(err, ExportError::CountMismatch { .. }),
            "attendu CountMismatch, obtenu: {err}"
        );
    }

    #[test]
    fn test_import_rejects_unknown_format() {
        let graph = KnowledgeGraph::new();
        let bytes = export_graph(&graph, ExportFormat::Json).unwrap();
        let mut v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        v["format"] = serde_json::Value::String("hakille-unknown".to_string());
        let bad = serde_json::to_vec(&v).unwrap();
        let err = import_graph(&bad).unwrap_err();
        assert!(
            matches!(err, ExportError::UnknownFormat { .. }),
            "attendu UnknownFormat, obtenu: {err}"
        );
    }

    #[test]
    fn test_export_cbor_is_explicit_error() {
        // CBOR tranché : variant conservé (compat API) mais backend absent —
        // erreur explicite, jamais de succès silencieux.
        let graph = KnowledgeGraph::new();
        let err = export_graph(&graph, ExportFormat::Cbor).unwrap_err();
        assert!(
            matches!(err, ExportError::CborNotSupported),
            "attendu CborNotSupported, obtenu: {err}"
        );
        assert!(err.to_string().contains("CBOR"));
    }

    #[test]
    fn test_export_metadata_spec_imp02() {
        // Spec IMP-02 : JSON {version, exported_at, triplets} + metadata
        // {version, exported_at, count}.
        let graph = KnowledgeGraph::new();
        let bytes = export_graph(&graph, ExportFormat::Json).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(v.get("exported_at").is_some());
        assert!(v.get("triplets").is_some());
        assert_eq!(v["metadata"]["version"], 1);
        assert_eq!(v["metadata"]["count"], 0);
        assert!(v["metadata"].get("exported_at").is_some());
        // Variante triplets directe.
        let triplets = import_graph_triplets(&bytes).unwrap();
        assert!(triplets.is_empty());
    }

    #[test]
    fn test_import_into_graph_counts_inserted_refused() {
        let mut src = KnowledgeGraph::new();
        let alice = src.resolve_subject("Alice");
        let aime = src.resolve_predicate("aime");
        let boit = src.resolve_predicate("boit");
        src.insert(sample_triplet_with_ids(alice.clone(), aime.clone(), "Rust"));
        src.insert(sample_triplet_with_ids(alice.clone(), boit.clone(), "Café"));
        let bytes = export_graph(&src, ExportFormat::Json).unwrap();

        // Import dans un graphe vide : tout inséré.
        let mut dst = KnowledgeGraph::new();
        let report = import_into_graph(&mut dst, &bytes).unwrap();
        assert_eq!(report.inserted, 2);
        assert_eq!(report.refused, 0);
        assert_eq!(dst.len(), 2);

        // Réimport du même export : doublons exacts S/P/O ⇒ idempotents
        // (réactivés, comptés insérés, taille inchangée).
        let report2 = import_into_graph(&mut dst, &bytes).unwrap();
        assert_eq!(report2.inserted, 2);
        assert_eq!(report2.refused, 0);
        assert_eq!(dst.len(), 2);
    }
}
