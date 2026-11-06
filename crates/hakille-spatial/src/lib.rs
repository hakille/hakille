//! # Hakille Spatial
//!
//! Primitives géométriques 3D, orientation de tête 6-DOF, squelette COCO 17 points,
//! classification de posture et rayon de regard pour le système cognitif Hakille.
//!
//! Ce crate respecte les principes directeurs de Hakille :
//! - 100% natif Rust sans dépendance C++ lourde ni Python.
//! - Zéro allocation dynamique pour les calculs géométriques fondamentaux (structures `Copy`).
//! - Sérialisation et désérialisation complètes via `serde`.

pub mod geometry;
pub mod head_pose;
pub mod skeleton;
pub mod gaze;

pub use geometry::{Point3D, Vector3D};
pub use head_pose::{CameraIntrinsics, HeadPose6D};
pub use skeleton::{BodySkeleton3D, CocoJointId, PostureKind, SkeletonJoint3D, COCO_KEYPOINTS_COUNT};
pub use gaze::{GazeRay3D, ScreenBounds};
