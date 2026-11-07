//! Squelette corporel 3D 17 points COCO ([`BodySkeleton3D`]) et classification de posture ([`PostureKind`]).

use crate::geometry::{Point3D, Vector3D};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Nombre standard de points clés du modèle COCO 17.
pub const COCO_KEYPOINTS_COUNT: usize = 17;

/// Énumération des 17 articulations / repères anatomiques du standard COCO Keypoints.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CocoJointId {
    Nose = 0,
    LeftEye = 1,
    RightEye = 2,
    LeftEar = 3,
    RightEar = 4,
    LeftShoulder = 5,
    RightShoulder = 6,
    LeftElbow = 7,
    RightElbow = 8,
    LeftWrist = 9,
    RightWrist = 10,
    LeftHip = 11,
    RightHip = 12,
    LeftKnee = 13,
    RightKnee = 14,
    LeftAnkle = 15,
    RightAnkle = 16,
}

impl CocoJointId {
    /// Convertit un entier `u8` (0..16) en identifiant d'articulation COCO.
    pub const fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::Nose),
            1 => Some(Self::LeftEye),
            2 => Some(Self::RightEye),
            3 => Some(Self::LeftEar),
            4 => Some(Self::RightEar),
            5 => Some(Self::LeftShoulder),
            6 => Some(Self::RightShoulder),
            7 => Some(Self::LeftElbow),
            8 => Some(Self::RightElbow),
            9 => Some(Self::LeftWrist),
            10 => Some(Self::RightWrist),
            11 => Some(Self::LeftHip),
            12 => Some(Self::RightHip),
            13 => Some(Self::LeftKnee),
            14 => Some(Self::RightKnee),
            15 => Some(Self::LeftAnkle),
            16 => Some(Self::RightAnkle),
            _ => None,
        }
    }

    /// Retourne l'indice `u8` associé.
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Nom lisible de l'articulation en chaîne statique.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Nose => "Nose",
            Self::LeftEye => "LeftEye",
            Self::RightEye => "RightEye",
            Self::LeftEar => "LeftEar",
            Self::RightEar => "RightEar",
            Self::LeftShoulder => "LeftShoulder",
            Self::RightShoulder => "RightShoulder",
            Self::LeftElbow => "LeftElbow",
            Self::RightElbow => "RightElbow",
            Self::LeftWrist => "LeftWrist",
            Self::RightWrist => "RightWrist",
            Self::LeftHip => "LeftHip",
            Self::RightHip => "RightHip",
            Self::LeftKnee => "LeftKnee",
            Self::RightKnee => "RightKnee",
            Self::LeftAnkle => "LeftAnkle",
            Self::RightAnkle => "RightAnkle",
        }
    }
}

impl fmt::Display for CocoJointId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Point d'articulation 3D détecté avec score de confiance.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SkeletonJoint3D {
    /// Identifiant de l'articulation (0..16 selon convention COCO).
    pub id: u8,
    /// Position 3D dans le repère caméra (en mètres).
    pub position: Point3D,
    /// Score de confiance de détection $\in [0.0, 1.0]$.
    pub confidence: f32,
}

impl SkeletonJoint3D {
    /// Seuil de confiance minimal recommandé par défaut pour filtrer les faux positifs.
    pub const DEFAULT_CONFIDENCE_THRESHOLD: f32 = 0.3;

    /// Crée une nouvelle articulation 3D.
    pub const fn new(id: u8, position: Point3D, confidence: f32) -> Self {
        Self {
            id,
            position,
            confidence,
        }
    }

    /// Vérifie si la confiance dépasse un seuil donné.
    pub fn is_confident(&self, threshold: f32) -> bool {
        self.confidence >= threshold
    }

    /// Retourne l'identifiant typé COCO correspondant, ou `None` si l'id est hors borne.
    pub fn joint_id(&self) -> Option<CocoJointId> {
        CocoJointId::from_u8(self.id)
    }
}

impl Default for SkeletonJoint3D {
    fn default() -> Self {
        Self {
            id: 0,
            position: Point3D::ORIGIN,
            confidence: 0.0,
        }
    }
}

/// Type de posture corporelle détectée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PostureKind {
    /// Posture droite et équilibrée.
    Upright,
    /// Posture voûtée / affalée (tête projetée vers l'avant, épaules rentrées).
    Slouched,
    /// Posture penchée vers l'avant (vers l'écran / bureau).
    LeaningForward,
    /// Posture inclinée vers l'arrière (adossé au dossier).
    Reclined,
    /// Utilisateur debout.
    Standing,
    /// Utilisateur assis.
    Seated,
}

impl PostureKind {
    /// Représentation en chaîne statique.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Upright => "Upright",
            Self::Slouched => "Slouched",
            Self::LeaningForward => "LeaningForward",
            Self::Reclined => "Reclined",
            Self::Standing => "Standing",
            Self::Seated => "Seated",
        }
    }
}

impl fmt::Display for PostureKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Squelette corporel complet composé des 17 points COCO et de la posture associée.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodySkeleton3D {
    /// Les 17 articulations ordonnées selon [`CocoJointId`].
    pub joints: [SkeletonJoint3D; 17],
    /// Classification de la posture actuelle.
    pub posture: PostureKind,
}

impl BodySkeleton3D {
    /// Crée un nouveau squelette à partir d'un tableau de 17 articulations et d'une posture.
    pub const fn new(joints: [SkeletonJoint3D; 17], posture: PostureKind) -> Self {
        Self { joints, posture }
    }

    /// Crée un squelette vide par défaut (articulations à l'origine avec confiance nulle).
    pub fn empty() -> Self {
        let mut joints = [SkeletonJoint3D::default(); 17];
        for i in 0..17 {
            joints[i].id = i as u8;
        }
        Self {
            joints,
            posture: PostureKind::Upright,
        }
    }

    /// Accès par référence à une articulation via son identifiant typé.
    pub fn joint(&self, id: CocoJointId) -> &SkeletonJoint3D {
        &self.joints[id as usize]
    }

    /// Accès mutable à une articulation via son identifiant typé.
    pub fn joint_mut(&mut self, id: CocoJointId) -> &mut SkeletonJoint3D {
        &mut self.joints[id as usize]
    }

    /// Vérifie si une articulation est visible avec une confiance suffisante.
    pub fn is_joint_visible(&self, id: CocoJointId, min_conf: f32) -> bool {
        self.joint(id).is_confident(min_conf)
    }

    /// Calcule le point milieu entre les deux épaules, si les deux sont visibles.
    pub fn mid_shoulders(&self, min_conf: f32) -> Option<Point3D> {
        let left = self.joint(CocoJointId::LeftShoulder);
        let right = self.joint(CocoJointId::RightShoulder);
        if left.is_confident(min_conf) && right.is_confident(min_conf) {
            Some(left.position.midpoint(&right.position))
        } else {
            None
        }
    }

    /// Calcule le point milieu entre les deux hanches, si les deux sont visibles.
    pub fn mid_hips(&self, min_conf: f32) -> Option<Point3D> {
        let left = self.joint(CocoJointId::LeftHip);
        let right = self.joint(CocoJointId::RightHip);
        if left.is_confident(min_conf) && right.is_confident(min_conf) {
            Some(left.position.midpoint(&right.position))
        } else {
            None
        }
    }

    /// Calcule le vecteur rachidien (colonne vertébrale) allant des hanches vers les épaules.
    pub fn spine_vector(&self, min_conf: f32) -> Option<Vector3D> {
        let shoulders = self.mid_shoulders(min_conf)?;
        let hips = self.mid_hips(min_conf)?;
        Some(shoulders - hips)
    }

    /// Calcule la largeur bi-acromiale (distance entre les deux épaules).
    pub fn shoulder_width(&self, min_conf: f32) -> Option<f32> {
        let left = self.joint(CocoJointId::LeftShoulder);
        let right = self.joint(CocoJointId::RightShoulder);
        if left.is_confident(min_conf) && right.is_confident(min_conf) {
            Some(left.position.distance_to(&right.position))
        } else {
            None
        }
    }

    /// Détermine la posture géométrique à partir des repères articulaires.
    ///
    /// Algorithme de classification souverain basé sur des seuils biomécaniques :
    /// 1. Si les membres inférieurs (genoux / chevilles) sont visibles et alignés verticalement : [`PostureKind::Standing`].
    /// 2. Si le bas du corps est plié à angle droit ou non visible (bureau / webcam) :
    ///    - Analyse de l'inclinaison sagittale de la colonne vertébrale ($\Delta Z$ épaules-hanches) :
    ///      * Si les épaules sont avancées vers la caméra ($\Delta Z < -0.10$ m ou angle $> 15^\circ$) : [`PostureKind::LeaningForward`].
    ///      * Si les épaules sont reculées ($\Delta Z > 0.10$ m ou angle $> 15^\circ$) : [`PostureKind::Reclined`].
    ///    - Analyse de la projection de la tête (Nez / Oreilles par rapport aux épaules) :
    ///      * Si le nez est projeté vers l'avant par rapport au plan des épaules ($\Delta Z_{\text{nose-shoulders}} < -0.08$ m) : [`PostureKind::Slouched`].
    ///    - Sinon : [`PostureKind::Upright`] (ou [`PostureKind::Seated`] selon contexte).
    pub fn classify_posture(&self, min_conf: f32) -> PostureKind {
        // 1. Détection de position debout si hanches et genoux sont clairement visibles
        let hips_visible = self.is_joint_visible(CocoJointId::LeftHip, min_conf)
            && self.is_joint_visible(CocoJointId::RightHip, min_conf);
        let knees_visible = self.is_joint_visible(CocoJointId::LeftKnee, min_conf)
            && self.is_joint_visible(CocoJointId::RightKnee, min_conf);

        if hips_visible && knees_visible {
            let mid_h = self.mid_hips(min_conf).unwrap();
            let mid_k = self
                .joint(CocoJointId::LeftKnee)
                .position
                .midpoint(&self.joint(CocoJointId::RightKnee).position);
            let dy = (mid_k.y - mid_h.y).abs();
            let dz = (mid_k.z - mid_h.z).abs();
            // Debout : distance verticale significative et cuisse presque verticale
            if dy > 0.35 && dz < 0.15 {
                return PostureKind::Standing;
            }
        }

        // 2. Analyse de la colonne et du buste (assis ou buste seul face caméra)
        if let (Some(shoulders), Some(hips)) =
            (self.mid_shoulders(min_conf), self.mid_hips(min_conf))
        {
            let dz_spine = shoulders.z - hips.z; // Négatif = épaules plus proches de la caméra que les hanches

            if dz_spine < -0.10 {
                return PostureKind::LeaningForward;
            }
            if dz_spine > 0.10 {
                return PostureKind::Reclined;
            }

            // Détection de posture voûtée (tête projetée vers l'avant par rapport aux épaules)
            let nose = self.joint(CocoJointId::Nose);
            if nose.is_confident(min_conf) {
                let dz_head = nose.position.z - shoulders.z;
                if dz_head < -0.08 {
                    return PostureKind::Slouched;
                }
            }

            return PostureKind::Upright;
        }

        // Si hanches non visibles (caméra cadrée serrée visage/épaules) : analyse tête/épaules
        if let Some(shoulders) = self.mid_shoulders(min_conf) {
            let nose = self.joint(CocoJointId::Nose);
            if nose.is_confident(min_conf) {
                let dz_head = nose.position.z - shoulders.z;
                if dz_head < -0.08 {
                    return PostureKind::Slouched;
                }
            }
        }

        PostureKind::Upright
    }
}

impl Default for BodySkeleton3D {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coco_joint_id_conversions() {
        for i in 0..17 {
            let joint = CocoJointId::from_u8(i).expect("All 0..16 must map to valid joints");
            assert_eq!(joint.as_u8(), i);
        }
        assert!(CocoJointId::from_u8(17).is_none());
        assert_eq!(CocoJointId::Nose.name(), "Nose");
        assert_eq!(CocoJointId::LeftAnkle.name(), "LeftAnkle");
    }

    #[test]
    fn test_mid_shoulders_and_hips() {
        let mut skel = BodySkeleton3D::empty();
        skel.joint_mut(CocoJointId::LeftShoulder).position = Point3D::new(-0.2, 0.3, 0.6);
        skel.joint_mut(CocoJointId::LeftShoulder).confidence = 0.9;
        skel.joint_mut(CocoJointId::RightShoulder).position = Point3D::new(0.2, 0.3, 0.6);
        skel.joint_mut(CocoJointId::RightShoulder).confidence = 0.9;

        let mid = skel
            .mid_shoulders(0.5)
            .expect("Should compute mid shoulders");
        assert!((mid.x).abs() < 1e-6);
        assert!((mid.y - 0.3).abs() < 1e-6);
        assert!((mid.z - 0.6).abs() < 1e-6);

        let width = skel.shoulder_width(0.5).expect("Should compute width");
        assert!((width - 0.4).abs() < 1e-6);

        // If one shoulder confidence is low, mid_shoulders returns None
        skel.joint_mut(CocoJointId::RightShoulder).confidence = 0.2;
        assert!(skel.mid_shoulders(0.5).is_none());
    }

    #[test]
    fn test_posture_classification_leaning_forward() {
        let mut skel = BodySkeleton3D::empty();
        // Épaules avancées (Z=0.45) par rapport aux hanches (Z=0.65)
        skel.joint_mut(CocoJointId::LeftShoulder).position = Point3D::new(-0.2, 0.3, 0.45);
        skel.joint_mut(CocoJointId::LeftShoulder).confidence = 0.9;
        skel.joint_mut(CocoJointId::RightShoulder).position = Point3D::new(0.2, 0.3, 0.45);
        skel.joint_mut(CocoJointId::RightShoulder).confidence = 0.9;

        skel.joint_mut(CocoJointId::LeftHip).position = Point3D::new(-0.15, 0.8, 0.65);
        skel.joint_mut(CocoJointId::LeftHip).confidence = 0.9;
        skel.joint_mut(CocoJointId::RightHip).position = Point3D::new(0.15, 0.8, 0.65);
        skel.joint_mut(CocoJointId::RightHip).confidence = 0.9;

        let posture = skel.classify_posture(0.5);
        assert_eq!(posture, PostureKind::LeaningForward);
    }

    #[test]
    fn test_posture_classification_slouched() {
        let mut skel = BodySkeleton3D::empty();
        // Colonne alignée verticalement mais tête (nez) très en avant des épaules
        skel.joint_mut(CocoJointId::LeftShoulder).position = Point3D::new(-0.2, 0.3, 0.6);
        skel.joint_mut(CocoJointId::LeftShoulder).confidence = 0.9;
        skel.joint_mut(CocoJointId::RightShoulder).position = Point3D::new(0.2, 0.3, 0.6);
        skel.joint_mut(CocoJointId::RightShoulder).confidence = 0.9;

        skel.joint_mut(CocoJointId::LeftHip).position = Point3D::new(-0.15, 0.8, 0.6);
        skel.joint_mut(CocoJointId::LeftHip).confidence = 0.9;
        skel.joint_mut(CocoJointId::RightHip).position = Point3D::new(0.15, 0.8, 0.6);
        skel.joint_mut(CocoJointId::RightHip).confidence = 0.9;

        skel.joint_mut(CocoJointId::Nose).position = Point3D::new(0.0, 0.1, 0.48); // 12 cm en avant des épaules
        skel.joint_mut(CocoJointId::Nose).confidence = 0.9;

        let posture = skel.classify_posture(0.5);
        assert_eq!(posture, PostureKind::Slouched);
    }

    #[test]
    fn test_posture_kind_display() {
        assert_eq!(PostureKind::Upright.to_string(), "Upright");
        assert_eq!(PostureKind::Slouched.to_string(), "Slouched");
        assert_eq!(PostureKind::LeaningForward.to_string(), "LeaningForward");
        assert_eq!(PostureKind::Reclined.to_string(), "Reclined");
        assert_eq!(PostureKind::Standing.to_string(), "Standing");
        assert_eq!(PostureKind::Seated.to_string(), "Seated");
    }
}
