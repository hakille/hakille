//! Graphe acyclique dirigé (DAG) de composition multimodale avec checkpoints et
//! lignée cryptographique (`parent_artifacts`).
//!
//! Exemple de pipeline complet (`StoryboardPreset::from_prompt`) :
//! ```text
//! [image: affiche clé] ─┐
//!                       ├─► [video: animation I2V] ─┐
//! [audio: narration] ───┤                           ├─► [mux: capsule finale]
//! [audio: ambiance]  ───┴─► [mix: sidechain duck] ──┘
//! ```
//!
//! Chaque nœud produit un artefact signé Ed25519 qui reste conservé dans le
//! `MediaStore` (checkpoint) ; les nœuds enfants héritent automatiquement des
//! SHA-256 de leurs parents dans leur manifeste de provenance.

use crate::error::{MediaError, MediaResult};
use crate::types::{
    AudioGenRequest, CameraMotion, ImageGenRequest, MediaArtifact, VideoContainer, VideoGenRequest,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// Action atomique exécutée par un nœud du DAG multimodal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum DagNodeKind {
    /// Génère une image clé.
    GenerateImage { request: ImageGenRequest },
    /// Génère une piste audio (voix, musique, ambiance ou Foley).
    GenerateAudio { request: AudioGenRequest },
    /// Génère une vidéo (optionnellement animée depuis l'image d'un nœud parent
    /// ou réactive à la piste audio d'un nœud parent).
    GenerateVideo {
        request: VideoGenRequest,
        #[serde(default)]
        source_image_node: Option<String>,
        #[serde(default)]
        audio_reactive_node: Option<String>,
    },
    /// Mixe deux pistes audio parentes (voix prioritaire + lit musical/ambiance
    /// avec sidechain ducking automatique).
    MixAudio {
        voice_node: String,
        bed_node: String,
        #[serde(default = "default_bed_gain")]
        bed_gain: f32,
        #[serde(default = "default_duck_gain")]
        duck_gain: f32,
    },
    /// Assemble une vidéo parente et une piste audio parente dans un conteneur
    /// final (`Mp4H264` ou `WebmVp9` via `FfmpegMuxer`).
    MuxAudioVideo {
        video_node: String,
        audio_node: String,
        #[serde(default = "default_mux_container")]
        container: VideoContainer,
    },
}

fn default_bed_gain() -> f32 {
    0.55
}
fn default_duck_gain() -> f32 {
    0.26
}
fn default_mux_container() -> VideoContainer {
    VideoContainer::Mp4H264
}

/// Nœud typé d'un graphe de composition multimodale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    pub id: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub kind: DagNodeKind,
}

/// Définition complète d'un pipeline multimodal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaPipelineDag {
    pub name: String,
    pub nodes: Vec<DagNode>,
}

/// Résultat complet d'une exécution DAG avec tous les artefacts intermédiaires
/// (checkpoints) et l'identifiant du nœud final.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagExecutionResult {
    pub pipeline_name: String,
    pub execution_order: Vec<String>,
    pub artifacts: HashMap<String, MediaArtifact>,
    pub final_node_id: String,
    pub total_elapsed_ms: u64,
}

impl MediaPipelineDag {
    /// Valide le graphe (nœuds uniques, dépendances existantes, absence de
    /// cycle) et renvoie l'ordre topologique déterministe (algorithme de Kahn).
    pub fn topological_order(&self) -> MediaResult<Vec<String>> {
        if self.nodes.is_empty() {
            return Err(MediaError::InvalidRequest(
                "le graphe DAG multimodal ne contient aucun nœud".into(),
            ));
        }
        let mut by_id: HashMap<&str, &DagNode> = HashMap::with_capacity(self.nodes.len());
        for node in &self.nodes {
            if node.id.trim().is_empty() {
                return Err(MediaError::InvalidRequest(
                    "identifiant de nœud DAG vide".into(),
                ));
            }
            if by_id.insert(node.id.as_str(), node).is_some() {
                return Err(MediaError::InvalidRequest(format!(
                    "identifiant de nœud DAG dupliqué : '{}'",
                    node.id
                )));
            }
        }

        // Rassemble les dépendances explicites ET implicites (références dans DagNodeKind).
        let mut indegree: HashMap<&str, usize> = HashMap::with_capacity(self.nodes.len());
        let mut dependents: HashMap<&str, Vec<&str>> = HashMap::with_capacity(self.nodes.len());
        for node in &self.nodes {
            let mut deps: HashSet<&str> = node.depends_on.iter().map(|s| s.as_str()).collect();
            for implicit in implicit_dependencies(&node.kind) {
                deps.insert(implicit);
            }
            for &dep in &deps {
                if !by_id.contains_key(dep) {
                    return Err(MediaError::InvalidRequest(format!(
                        "le nœud '{}' dépend du nœud inconnu '{dep}'",
                        node.id
                    )));
                }
                if dep == node.id.as_str() {
                    return Err(MediaError::DagCycleDetected(format!(
                        "auto-dépendance sur le nœud '{}'",
                        node.id
                    )));
                }
                dependents.entry(dep).or_default().push(node.id.as_str());
            }
            indegree.insert(node.id.as_str(), deps.len());
        }

        let mut ready: VecDeque<&str> = self
            .nodes
            .iter()
            .filter(|n| indegree[n.id.as_str()] == 0)
            .map(|n| n.id.as_str())
            .collect();

        let mut order = Vec::with_capacity(self.nodes.len());
        while let Some(curr) = ready.pop_front() {
            order.push(curr.to_string());
            if let Some(children) = dependents.get(curr) {
                for &child in children {
                    let deg = indegree.get_mut(child).expect("nœud présent");
                    *deg -= 1;
                    if *deg == 0 {
                        ready.push_back(child);
                    }
                }
            }
        }

        if order.len() != self.nodes.len() {
            let remaining: Vec<String> = self
                .nodes
                .iter()
                .filter(|n| !order.contains(&n.id))
                .map(|n| n.id.clone())
                .collect();
            return Err(MediaError::DagCycleDetected(format!(
                "nœuds pris dans un cycle : {remaining:?}"
            )));
        }

        Ok(order)
    }

    pub fn node_by_id(&self, id: &str) -> Option<&DagNode> {
        self.nodes.iter().find(|n| n.id == id)
    }
}

/// Dépendances implicites référencées dans la charge utile d'un nœud.
pub fn implicit_dependencies(kind: &DagNodeKind) -> Vec<&str> {
    match kind {
        DagNodeKind::GenerateImage { .. } | DagNodeKind::GenerateAudio { .. } => Vec::new(),
        DagNodeKind::GenerateVideo {
            source_image_node,
            audio_reactive_node,
            ..
        } => {
            let mut v = Vec::new();
            if let Some(s) = source_image_node {
                v.push(s.as_str());
            }
            if let Some(a) = audio_reactive_node {
                v.push(a.as_str());
            }
            v
        }
        DagNodeKind::MixAudio {
            voice_node,
            bed_node,
            ..
        } => vec![voice_node.as_str(), bed_node.as_str()],
        DagNodeKind::MuxAudioVideo {
            video_node,
            audio_node,
            ..
        } => vec![video_node.as_str(), audio_node.as_str()],
    }
}

/// Générateur de pipeline clé-en-main pour `:gen-story <thème>` :
/// produit une image clé, une narration vocale, un lit d'ambiance, un mix audio
/// avec sidechain ducking, une vidéo animée Ken Burns et (si `ffmpeg` est
/// présent ou demandé) une capsule finale muxée MP4 H.264 + AAC.
pub struct StoryboardPreset;

impl StoryboardPreset {
    pub fn from_prompt(
        prompt: &str,
        width: u32,
        height: u32,
        fps: u16,
        duration_secs: f32,
        seed: Option<u64>,
        include_ffmpeg_mux: bool,
    ) -> MediaPipelineDag {
        let dur = duration_secs.clamp(1.5, 20.0);
        let num_frames = ((dur * f32::from(fps.max(4))).round() as u32).clamp(8, 240);
        let w = (width / 8).max(16) * 8;
        let h = (height / 8).max(16) * 8;

        let mut img_req = ImageGenRequest::simple(prompt);
        img_req.width = w;
        img_req.height = h;
        img_req.steps = 12;
        img_req.seed = seed;
        img_req.engine = "auto".into();

        let mut voice_req = AudioGenRequest::speech(prompt);
        voice_req.duration_secs = dur;
        voice_req.seed = seed;
        voice_req.engine = "auto".into();

        let mut bed_req = AudioGenRequest::soundscape(prompt, dur);
        bed_req.seed = seed.map(|s| s.wrapping_add(1));
        bed_req.engine = "procedural-dsp".into();

        let mut vid_req = VideoGenRequest::simple(prompt);
        vid_req.width = w;
        vid_req.height = h;
        vid_req.fps = fps.max(4);
        vid_req.num_frames = num_frames;
        vid_req.camera_motion = CameraMotion::KenBurns;
        vid_req.container = if include_ffmpeg_mux {
            VideoContainer::Y4m
        } else {
            VideoContainer::Apng
        };
        vid_req.seed = seed;
        vid_req.engine = "auto".into();

        let mut nodes = vec![
            DagNode {
                id: "keyframe".into(),
                depends_on: vec![],
                kind: DagNodeKind::GenerateImage { request: img_req },
            },
            DagNode {
                id: "narration".into(),
                depends_on: vec![],
                kind: DagNodeKind::GenerateAudio { request: voice_req },
            },
            DagNode {
                id: "ambiance".into(),
                depends_on: vec![],
                kind: DagNodeKind::GenerateAudio { request: bed_req },
            },
            DagNode {
                id: "audio_mix".into(),
                depends_on: vec!["narration".into(), "ambiance".into()],
                kind: DagNodeKind::MixAudio {
                    voice_node: "narration".into(),
                    bed_node: "ambiance".into(),
                    bed_gain: 0.45,
                    duck_gain: 0.25,
                },
            },
            DagNode {
                id: "video_clip".into(),
                depends_on: vec!["keyframe".into()],
                kind: DagNodeKind::GenerateVideo {
                    request: vid_req,
                    source_image_node: Some("keyframe".into()),
                    audio_reactive_node: None,
                },
            },
        ];

        if include_ffmpeg_mux {
            nodes.push(DagNode {
                id: "capsule_finale".into(),
                depends_on: vec!["video_clip".into(), "audio_mix".into()],
                kind: DagNodeKind::MuxAudioVideo {
                    video_node: "video_clip".into(),
                    audio_node: "audio_mix".into(),
                    container: VideoContainer::Mp4H264,
                },
            });
        }

        MediaPipelineDag {
            name: format!("storyboard:{}", prompt.chars().take(48).collect::<String>()),
            nodes,
        }
    }
}
