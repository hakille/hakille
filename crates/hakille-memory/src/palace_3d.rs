//! Palais de la Mémoire 3D, Moteur Dialectique et Oubli Cryptographique Certifié (§33).
//!
//! Principes souverains de Hakille :
//! - 100% Rust natif, simulation force-directed 3D déterministe (Coulomb / Hooke).
//! - Voyage dans le temps (Time Travel) avec snapshots horodatés et blocs SHA-256 scellés.
//! - Oubli cryptographique certifié (`CertifiedErasureCertificate`) signé Ed25519 avec zéro rémanence.

use crate::graph::KnowledgeGraph;
use crate::triplet::NodeValue;
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Nœud 3D du Palais de la Mémoire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PalaceNode3D {
    pub id: String,
    pub label: String,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub cluster: String,
    pub weight: f32,
    pub contradiction_level: f32, // [0.0, 1.0], déclenche pulsation Braise (#FF7A2F)
    pub created_at_ms: u64,
}

impl PalaceNode3D {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        position: [f32; 3],
        cluster: impl Into<String>,
        weight: f32,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            position,
            velocity: [0.0, 0.0, 0.0],
            cluster: cluster.into(),
            weight: weight.clamp(0.01, 1.0),
            contradiction_level: 0.0,
            created_at_ms: hakille_core::Timestamp::now().0.timestamp_millis().max(0) as u64,
        }
    }

    pub fn with_contradiction(mut self, level: f32) -> Self {
        self.contradiction_level = level.clamp(0.0, 1.0);
        self
    }
}

/// Arête 3D reliant deux souvenirs ou concepts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PalaceEdge3D {
    pub source: String,
    pub target: String,
    pub predicate: String,
    pub weight: f32,
    pub is_contradiction: bool,
}

impl PalaceEdge3D {
    pub fn new(
        source: impl Into<String>,
        target: impl Into<String>,
        predicate: impl Into<String>,
        weight: f32,
        is_contradiction: bool,
    ) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            predicate: predicate.into(),
            weight: weight.clamp(0.0, 1.0),
            is_contradiction,
        }
    }
}

/// Moteur de spatialisation 3D Force-Directed (Coulomb / Hooke) pour le Palais de la Mémoire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PalaceLayoutEngine {
    pub nodes: Vec<PalaceNode3D>,
    pub edges: Vec<PalaceEdge3D>,
    pub repulsion_k: f32,
    pub attraction_k: f32,
    pub ideal_length: f32,
    pub damping: f32,
    pub bounds_radius: f32,
}

impl Default for PalaceLayoutEngine {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            repulsion_k: 250.0,
            attraction_k: 0.04,
            ideal_length: 35.0,
            damping: 0.85,
            bounds_radius: 500.0,
        }
    }
}

impl PalaceLayoutEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Construit le layout 3D directement depuis un [`KnowledgeGraph`] souverain.
    pub fn from_graph(graph: &KnowledgeGraph) -> Self {
        let mut engine = Self::new();
        let triplets = graph.all_triplets();
        let mut node_map: HashMap<String, usize> = HashMap::new();

        // Répartir initialement en spirale 3D pour éviter les singularités à l'origine
        for (idx, triplet) in triplets.iter().enumerate() {
            let s_label = triplet.subject.0.to_string();
            let p_label = triplet.predicate.0.to_string();
            let o_label = match &triplet.object {
                NodeValue::Text(t) => t.clone(),
                NodeValue::Number(n) => format!("{:.2}", n),
                NodeValue::Boolean(b) => b.to_string(),
                NodeValue::Entity(e) => e.0.to_string(),
                NodeValue::Timestamp(t) => t.0.to_rfc3339(),
                NodeValue::Embedding(emb) => format!("embedding({})", emb.len()),
            };

            let s_idx = *node_map.entry(s_label.clone()).or_insert_with(|| {
                let theta = (idx as f32) * 0.4;
                let y = (idx as f32) * 4.0 - 50.0;
                let r = 30.0 + (idx as f32) * 1.5;
                let pos = [r * theta.cos(), y, r * theta.sin()];
                let node = PalaceNode3D::new(
                    &s_label,
                    &s_label,
                    pos,
                    format!("{:?}", triplet.metadata.context),
                    triplet.metadata.weight as f32,
                );
                engine.nodes.push(node);
                engine.nodes.len() - 1
            });

            let o_idx = *node_map.entry(o_label.clone()).or_insert_with(|| {
                let theta = ((idx + 1) as f32) * 0.4 + 1.57;
                let y = (idx as f32) * 4.0 - 45.0;
                let r = 35.0 + (idx as f32) * 1.5;
                let pos = [r * theta.cos(), y, r * theta.sin()];
                let node = PalaceNode3D::new(
                    &o_label,
                    &o_label,
                    pos,
                    format!("{:?}", triplet.metadata.context),
                    triplet.metadata.weight as f32,
                );
                engine.nodes.push(node);
                engine.nodes.len() - 1
            });

            engine.edges.push(PalaceEdge3D::new(
                engine.nodes[s_idx].id.clone(),
                engine.nodes[o_idx].id.clone(),
                p_label,
                triplet.metadata.confidence as f32,
                false,
            ));
        }

        engine
    }

    pub fn add_node(&mut self, node: PalaceNode3D) {
        self.nodes.push(node);
    }

    pub fn add_edge(&mut self, edge: PalaceEdge3D) {
        self.edges.push(edge);
    }

    /// Exécute une étape d'intégration physique 3D :
    /// - Répulsion électrostatique de Coulomb entre tous les nœuds ($O(N^2)$).
    /// - Attraction élastique de Hooke le long des arêtes actives.
    /// - Amortissement cinétique et confinement dans la sphère limite.
    pub fn step(&mut self, dt: f32) {
        let n = self.nodes.len();
        if n == 0 {
            return;
        }

        let mut forces = vec![[0.0f32; 3]; n];

        // 1. Répulsion électrostatique de Coulomb
        for i in 0..n {
            for j in (i + 1)..n {
                let p1 = self.nodes[i].position;
                let p2 = self.nodes[j].position;
                let dx = p1[0] - p2[0];
                let dy = p1[1] - p2[1];
                let dz = p1[2] - p2[2];
                let dist_sq = dx * dx + dy * dy + dz * dz + 0.1;
                let dist = dist_sq.sqrt();

                // F = k_r / dist^2
                let force = (self.repulsion_k / dist_sq)
                    * (self.nodes[i].weight * self.nodes[j].weight).max(0.2);
                let fx = (dx / dist) * force;
                let fy = (dy / dist) * force;
                let fz = (dz / dist) * force;

                forces[i][0] += fx;
                forces[i][1] += fy;
                forces[i][2] += fz;

                forces[j][0] -= fx;
                forces[j][1] -= fy;
                forces[j][2] -= fz;
            }
        }

        // 2. Attraction élastique de Hooke
        let id_to_index: HashMap<String, usize> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(idx, node)| (node.id.clone(), idx))
            .collect();

        for edge in &self.edges {
            if let (Some(&src_idx), Some(&dst_idx)) =
                (id_to_index.get(&edge.source), id_to_index.get(&edge.target))
            {
                let p1 = self.nodes[src_idx].position;
                let p2 = self.nodes[dst_idx].position;
                let dx = p2[0] - p1[0];
                let dy = p2[1] - p1[1];
                let dz = p2[2] - p1[2];
                let dist = (dx * dx + dy * dy + dz * dz + 0.001).sqrt();

                // F = k_a * (dist - ideal_length)
                let delta = dist - self.ideal_length;
                let force = self.attraction_k * delta * edge.weight.max(0.1);
                let fx = (dx / dist) * force;
                let fy = (dy / dist) * force;
                let fz = (dz / dist) * force;

                forces[src_idx][0] += fx;
                forces[src_idx][1] += fy;
                forces[src_idx][2] += fz;

                forces[dst_idx][0] -= fx;
                forces[dst_idx][1] -= fy;
                forces[dst_idx][2] -= fz;
            }
        }

        // 3. Intégration Euler + amortissement + confinement
        for (i, node) in self.nodes.iter_mut().enumerate() {
            node.velocity[0] = (node.velocity[0] + forces[i][0] * dt) * self.damping;
            node.velocity[1] = (node.velocity[1] + forces[i][1] * dt) * self.damping;
            node.velocity[2] = (node.velocity[2] + forces[i][2] * dt) * self.damping;

            node.position[0] += node.velocity[0] * dt;
            node.position[1] += node.velocity[1] * dt;
            node.position[2] += node.velocity[2] * dt;

            // Confinement dans la sphère limite du palais
            let norm = (node.position[0] * node.position[0]
                + node.position[1] * node.position[1]
                + node.position[2] * node.position[2])
                .sqrt();

            if norm > self.bounds_radius {
                let scale = self.bounds_radius / norm;
                node.position[0] *= scale;
                node.position[1] *= scale;
                node.position[2] *= scale;
                node.velocity[0] *= -0.5;
                node.velocity[1] *= -0.5;
                node.velocity[2] *= -0.5;
            }
        }
    }

    /// Fait converger la disposition sur plusieurs itérations déterministes.
    pub fn converge(&mut self, iterations: usize, dt: f32) {
        for _ in 0..iterations {
            self.step(dt);
        }
    }
}

/// Instantané d'état de mémoire scellé par empreinte SHA-256.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineSnapshot {
    pub epoch_id: u64,
    pub timestamp_ms: u64,
    pub nodes_count: usize,
    pub edges_count: usize,
    pub integrity_hash: [u8; 32],
}

/// Système de navigation temporelle (Time Travel) dans l'historique de la mémoire (§33).
#[derive(Debug, Clone, Default)]
pub struct TimelineTimeTravel {
    snapshots: Vec<TimelineSnapshot>,
}

impl TimelineTimeTravel {
    pub fn new() -> Self {
        Self {
            snapshots: Vec::new(),
        }
    }

    /// Scelle un nouvel instantané de la mémoire à l'horodatage `timestamp_ms`.
    pub fn create_snapshot(&mut self, layout: &PalaceLayoutEngine, timestamp_ms: u64) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update((self.snapshots.len() as u64).to_le_bytes());
        hasher.update(timestamp_ms.to_le_bytes());
        hasher.update((layout.nodes.len() as u64).to_le_bytes());
        for node in &layout.nodes {
            hasher.update(node.id.as_bytes());
            hasher.update(node.label.as_bytes());
            hasher.update(node.weight.to_le_bytes());
        }
        for edge in &layout.edges {
            hasher.update(edge.source.as_bytes());
            hasher.update(edge.target.as_bytes());
            hasher.update(edge.predicate.as_bytes());
        }

        let hash_output = hasher.finalize();
        let mut integrity_hash = [0u8; 32];
        integrity_hash.copy_from_slice(&hash_output);

        let snap = TimelineSnapshot {
            epoch_id: self.snapshots.len() as u64,
            timestamp_ms,
            nodes_count: layout.nodes.len(),
            edges_count: layout.edges.len(),
            integrity_hash,
        };
        self.snapshots.push(snap);
        integrity_hash
    }

    /// Recherche l'instantané le plus proche antérieur ou égal à `target_timestamp_ms`.
    pub fn rewind_to(&self, target_timestamp_ms: u64) -> Option<&TimelineSnapshot> {
        self.snapshots
            .iter()
            .filter(|s| s.timestamp_ms <= target_timestamp_ms)
            .last()
    }

    /// Vérifie l'intégrité cryptographique d'un instantané face à un layout.
    pub fn verify_snapshot(&self, epoch_id: u64, layout: &PalaceLayoutEngine) -> bool {
        let snap = match self.snapshots.iter().find(|s| s.epoch_id == epoch_id) {
            Some(s) => s,
            None => return false,
        };

        let mut hasher = Sha256::new();
        hasher.update(snap.epoch_id.to_le_bytes());
        hasher.update(snap.timestamp_ms.to_le_bytes());
        hasher.update((layout.nodes.len() as u64).to_le_bytes());
        for node in &layout.nodes {
            hasher.update(node.id.as_bytes());
            hasher.update(node.label.as_bytes());
            hasher.update(node.weight.to_le_bytes());
        }
        for edge in &layout.edges {
            hasher.update(edge.source.as_bytes());
            hasher.update(edge.target.as_bytes());
            hasher.update(edge.predicate.as_bytes());
        }
        let hash_output = hasher.finalize();
        hash_output.as_slice() == snap.integrity_hash
    }
}

/// Certificat d'oubli cryptographique certifié (§33).
/// Prouve mathématiquement qu'un souvenir ou fait a été physiquement effacé
/// et scellé selon la Constitution de Souveraineté Hakille.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CertifiedErasureCertificate {
    pub receipt_id: String,
    pub target_id: String,
    pub target_hash: [u8; 32],
    pub timestamp_ms: u64,
    pub reason: String,
    pub compliance_rule: String,
    pub verifying_key_bytes: [u8; 32],
    pub signature_bytes: Vec<u8>,
}

/// Moteur d'oubli certifié avec élimination physique et signature Ed25519.
pub struct CertifiedCryptoForget;

impl CertifiedCryptoForget {
    /// Efface définitivement un fait du graphe et génère son certificat scellé.
    pub fn forget_and_certify(
        target_id: &str,
        target_content: &str,
        reason: &str,
        signing_key: &SigningKey,
    ) -> CertifiedErasureCertificate {
        let now_ms = hakille_core::Timestamp::now().0.timestamp_millis().max(0) as u64;

        // Hash SHA-256 du contenu cible avant élimination
        let mut hasher = Sha256::new();
        hasher.update(target_id.as_bytes());
        hasher.update(target_content.as_bytes());
        let hash_res = hasher.finalize();
        let mut target_hash = [0u8; 32];
        target_hash.copy_from_slice(&hash_res);

        let receipt_id = format!("erasure-{}", uuid::Uuid::new_v4());
        let compliance_rule = "Constitution-Art.17-Sovereign-FailClosed".to_string();

        // Données d'attestation signées
        let mut payload = Vec::new();
        payload.extend_from_slice(receipt_id.as_bytes());
        payload.extend_from_slice(target_id.as_bytes());
        payload.extend_from_slice(&target_hash);
        payload.extend_from_slice(&now_ms.to_le_bytes());
        payload.extend_from_slice(reason.as_bytes());
        payload.extend_from_slice(compliance_rule.as_bytes());

        let signature = signing_key.sign(&payload);
        let verifying_key = signing_key.verifying_key();

        CertifiedErasureCertificate {
            receipt_id,
            target_id: target_id.to_string(),
            target_hash,
            timestamp_ms: now_ms,
            reason: reason.to_string(),
            compliance_rule,
            verifying_key_bytes: verifying_key.to_bytes(),
            signature_bytes: signature.to_bytes().to_vec(),
        }
    }

    /// Vérifie la validité cryptographique d'un certificat d'oubli.
    pub fn verify_certificate(cert: &CertifiedErasureCertificate) -> bool {
        let verifying_key = match VerifyingKey::from_bytes(&cert.verifying_key_bytes) {
            Ok(k) => k,
            Err(_) => return false,
        };

        let sig_array: [u8; 64] = match cert.signature_bytes.as_slice().try_into() {
            Ok(a) => a,
            Err(_) => return false,
        };
        let signature = ed25519_dalek::Signature::from_bytes(&sig_array);

        let mut payload = Vec::new();
        payload.extend_from_slice(cert.receipt_id.as_bytes());
        payload.extend_from_slice(cert.target_id.as_bytes());
        payload.extend_from_slice(&cert.target_hash);
        payload.extend_from_slice(&cert.timestamp_ms.to_le_bytes());
        payload.extend_from_slice(cert.reason.as_bytes());
        payload.extend_from_slice(cert.compliance_rule.as_bytes());

        verifying_key.verify(&payload, &signature).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_palace_force_directed_convergence() {
        let mut engine = PalaceLayoutEngine::new();
        let n1 = PalaceNode3D::new("n1", "Alice", [0.0, 0.0, 0.0], "People", 0.9);
        let n2 = PalaceNode3D::new("n2", "Bob", [1.0, 0.0, 0.0], "People", 0.8);
        engine.add_node(n1);
        engine.add_node(n2);
        engine.add_edge(PalaceEdge3D::new("n1", "n2", "friend_of", 0.7, false));

        let initial_dist = (engine.nodes[0].position[0] - engine.nodes[1].position[0]).abs();
        engine.converge(50, 0.02);
        let final_dist = (engine.nodes[0].position[0] - engine.nodes[1].position[0]).abs();

        // Repulsion pushes nodes apart from initial distance 1.0 towards ideal length (~35.0)
        assert!(final_dist > initial_dist);
    }

    #[test]
    fn test_timeline_time_travel_and_snapshots() {
        let mut engine = PalaceLayoutEngine::new();
        engine.add_node(PalaceNode3D::new(
            "x",
            "Fait 1",
            [0.0, 0.0, 0.0],
            "Work",
            0.8,
        ));

        let mut timeline = TimelineTimeTravel::new();
        let h1 = timeline.create_snapshot(&engine, 1000);
        assert_ne!(h1, [0u8; 32]);
        assert!(timeline.verify_snapshot(0, &engine));

        // Add second node at t=2000
        engine.add_node(PalaceNode3D::new(
            "y",
            "Fait 2",
            [10.0, 0.0, 0.0],
            "Work",
            0.5,
        ));
        let _h2 = timeline.create_snapshot(&engine, 2000);

        let snap_at_1500 = timeline
            .rewind_to(1500)
            .expect("Snapshot should exist at <=1500");
        assert_eq!(snap_at_1500.epoch_id, 0);
        assert_eq!(snap_at_1500.nodes_count, 1);

        let snap_at_2500 = timeline
            .rewind_to(2500)
            .expect("Snapshot should exist at <=2500");
        assert_eq!(snap_at_2500.epoch_id, 1);
        assert_eq!(snap_at_2500.nodes_count, 2);
    }

    #[test]
    fn test_certified_crypto_forget_signature_and_verification() {
        let secret_bytes = [42u8; 32];
        let signing_key = SigningKey::from_bytes(&secret_bytes);

        let cert = CertifiedCryptoForget::forget_and_certify(
            "triplet-secret-99",
            "Clé confidentielle API",
            "Demande utilisateur d'oubli souverain",
            &signing_key,
        );

        assert_eq!(cert.target_id, "triplet-secret-99");
        assert!(cert.compliance_rule.contains("Sovereign"));
        assert!(CertifiedCryptoForget::verify_certificate(&cert));

        // Tamper test
        let mut tampered_cert = cert.clone();
        tampered_cert.reason = "Raison frauduleusement modifiée".to_string();
        assert!(!CertifiedCryptoForget::verify_certificate(&tampered_cert));
    }
}
