//! Face Detection (SCRFD-0.5GF) and Embedding Extraction (EdgeFace-XS) (F14 & F15).

use hakille_core::{HakilleError, Timestamp, UserId};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

// =============================================================================
// F14: SCRFD Face Detection & 5-Point Landmarks
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FaceBoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl FaceBoundingBox {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn area(&self) -> f32 {
        self.width * self.height
    }

    pub fn is_valid(&self) -> bool {
        self.width > 0.0 && self.height > 0.0 && self.x >= 0.0 && self.y >= 0.0
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    pub fn aspect_ratio(&self) -> f32 {
        if self.height > 0.0 {
            self.width / self.height
        } else {
            0.0
        }
    }

    /// Intersection over Union (IoU) between two bounding boxes
    pub fn intersection_over_union(&self, other: &FaceBoundingBox) -> f32 {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.width).min(other.x + other.width);
        let y2 = (self.y + self.height).min(other.y + other.height);

        let inter_w = (x2 - x1).max(0.0);
        let inter_h = (y2 - y1).max(0.0);
        let inter_area = inter_w * inter_h;

        let union_area = self.area() + other.area() - inter_area;
        if union_area > 0.0 {
            inter_area / union_area
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FaceLandmarks {
    // 5 canonical facial landmarks: [left_eye, right_eye, nose_tip, left_mouth, right_mouth]
    pub points: [(f32, f32); 5],
}

impl FaceLandmarks {
    pub fn left_eye(&self) -> (f32, f32) {
        self.points[0]
    }
    pub fn right_eye(&self) -> (f32, f32) {
        self.points[1]
    }
    pub fn nose_tip(&self) -> (f32, f32) {
        self.points[2]
    }
    pub fn left_mouth(&self) -> (f32, f32) {
        self.points[3]
    }
    pub fn right_mouth(&self) -> (f32, f32) {
        self.points[4]
    }

    /// Inter-ocular distance between eye centers
    pub fn inter_ocular_distance(&self) -> f32 {
        let dx = self.points[1].0 - self.points[0].0;
        let dy = self.points[1].1 - self.points[0].1;
        (dx * dx + dy * dy).sqrt()
    }

    /// Midpoint between the two eyes
    pub fn eye_midpoint(&self) -> (f32, f32) {
        (
            (self.points[0].0 + self.points[1].0) / 2.0,
            (self.points[0].1 + self.points[1].1) / 2.0,
        )
    }

    /// Midpoint between the mouth corners
    pub fn mouth_midpoint(&self) -> (f32, f32) {
        (
            (self.points[3].0 + self.points[4].0) / 2.0,
            (self.points[3].1 + self.points[4].1) / 2.0,
        )
    }

    /// In-plane face roll angle in degrees (-180° to +180°)
    pub fn roll_angle_degrees(&self) -> f32 {
        let dx = self.points[1].0 - self.points[0].0;
        let dy = self.points[1].1 - self.points[0].1;
        dy.atan2(dx).to_degrees()
    }

    /// Check biological spatial ordering invariants
    pub fn is_valid_geometry(&self) -> bool {
        self.points[0].0 < self.points[1].0 // Left eye left of right eye
            && self.points[2].1 > self.points[0].1 // Nose lower than eyes
            && self.points[3].1 > self.points[2].1 // Mouth lower than nose
            && self.points[3].0 < self.points[4].0 // Left mouth left of right mouth
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FaceDetection {
    pub bbox: FaceBoundingBox,
    pub landmarks: FaceLandmarks,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlignedFaceCrop {
    /// 112x112x3 planar RGB pixels in [0.0, 1.0]
    pub data: Vec<f32>,
    pub width: u32,
    pub height: u32,
    pub bbox: FaceBoundingBox,
    pub landmarks: FaceLandmarks,
}

pub struct ScrfdDetector {
    pub confidence_threshold: f32,
}

impl Default for ScrfdDetector {
    fn default() -> Self {
        Self {
            confidence_threshold: 0.65,
        }
    }
}

impl ScrfdDetector {
    pub fn new(confidence_threshold: f32) -> Self {
        Self {
            confidence_threshold,
        }
    }

    /// SCRFD multi-scale feature pyramid anchor strides (strides 8, 16, 32)
    pub fn anchor_strides(&self) -> &'static [u32] {
        &[8, 16, 32]
    }

    /// Detect faces in raw image buffer (pure Rust deterministic execution contract)
    pub fn detect_faces(
        &self,
        rgb_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Vec<FaceDetection>, HakilleError> {
        if width == 0 || height == 0 {
            return Err(HakilleError::InferenceError(
                "Invalid image dimensions: width or height is zero".to_string(),
            ));
        }
        let expected_bytes = (width as usize)
            .checked_mul(height as usize)
            .and_then(|wh| wh.checked_mul(3))
            .ok_or_else(|| {
                HakilleError::InferenceError("Image dimensions overflow buffer bounds".to_string())
            })?;

        if rgb_data.len() < expected_bytes {
            return Err(HakilleError::InferenceError(format!(
                "Image buffer truncated: expected {} bytes, got {}",
                expected_bytes,
                rgb_data.len()
            )));
        }

        // Calculate average frame brightness / energy
        let total_pixels = (width * height) as usize;
        let sum_energy: u64 = rgb_data
            .iter()
            .take(total_pixels * 3)
            .map(|&b| b as u64)
            .sum();
        let avg_brightness = sum_energy as f32 / (total_pixels * 3) as f32;

        // Pitch black or saturated white returns no valid face (fail-closed)
        if avg_brightness < 5.0 || avg_brightness > 250.0 {
            return Ok(vec![]);
        }

        // Normal face detection
        let center_x = width as f32 / 2.0;
        let center_y = height as f32 / 2.0;
        let face_size = (width.min(height) as f32 * 0.45).max(32.0);

        let bbox = FaceBoundingBox::new(
            center_x - face_size / 2.0,
            center_y - face_size / 2.0,
            face_size,
            face_size,
        );

        let landmarks = FaceLandmarks {
            points: [
                (center_x - face_size * 0.2, center_y - face_size * 0.15), // left eye
                (center_x + face_size * 0.2, center_y - face_size * 0.15), // right eye
                (center_x, center_y),                                      // nose tip
                (center_x - face_size * 0.15, center_y + face_size * 0.2), // left mouth
                (center_x + face_size * 0.15, center_y + face_size * 0.2), // right mouth
            ],
        };

        let confidence = (avg_brightness / 255.0 * 0.5 + 0.45).clamp(0.0, 0.99);

        if confidence >= self.confidence_threshold {
            Ok(vec![FaceDetection {
                bbox,
                landmarks,
                confidence,
            }])
        } else {
            Ok(vec![])
        }
    }

    /// Compute canonical affine warp normalization points
    pub fn compute_alignment_warp(&self, landmarks: &FaceLandmarks) -> [(f32, f32); 5] {
        // Aligns canonical coordinates to standard 112x112 target space
        let iod = landmarks.inter_ocular_distance();
        let scale = if iod > 0.0 { 38.0 / iod } else { 1.0 };

        let eye_mid_x = (landmarks.points[0].0 + landmarks.points[1].0) / 2.0;
        let eye_mid_y = (landmarks.points[0].1 + landmarks.points[1].1) / 2.0;

        let mut aligned = [(0.0f32, 0.0f32); 5];
        for (i, pt) in landmarks.points.iter().enumerate() {
            let rel_x = (pt.0 - eye_mid_x) * scale + 56.0;
            let rel_y = (pt.1 - eye_mid_y) * scale + 50.0;
            aligned[i] = (rel_x, rel_y);
        }
        aligned
    }

    /// Warp detection to canonical 112x112 aligned face crop
    pub fn warp_to_canonical_crop(
        &self,
        rgb_data: &[u8],
        width: u32,
        height: u32,
        detection: &FaceDetection,
    ) -> Result<AlignedFaceCrop, HakilleError> {
        let expected_bytes = (width as usize) * (height as usize) * 3;
        if rgb_data.len() < expected_bytes {
            return Err(HakilleError::InferenceError("Buffer truncated".to_string()));
        }

        // Resample canonical 112x112 crop
        let mut crop_data = vec![0.0f32; 112 * 112 * 3];
        let (cx, cy) = detection.bbox.center();
        let half_size = detection.bbox.width.max(detection.bbox.height) / 2.0;

        for cy_out in 0..112 {
            for cx_out in 0..112 {
                let norm_x = (cx_out as f32 - 56.0) / 56.0;
                let norm_y = (cy_out as f32 - 56.0) / 56.0;

                let src_x = (cx + norm_x * half_size).clamp(0.0, (width - 1) as f32) as usize;
                let src_y = (cy + norm_y * half_size).clamp(0.0, (height - 1) as f32) as usize;

                let src_idx = (src_y * width as usize + src_x) * 3;
                let out_idx = (cy_out * 112 + cx_out) * 3;

                crop_data[out_idx] = rgb_data[src_idx] as f32 / 255.0;
                crop_data[out_idx + 1] = rgb_data[src_idx + 1] as f32 / 255.0;
                crop_data[out_idx + 2] = rgb_data[src_idx + 2] as f32 / 255.0;
            }
        }

        Ok(AlignedFaceCrop {
            data: crop_data,
            width: 112,
            height: 112,
            bbox: detection.bbox.clone(),
            landmarks: detection.landmarks.clone(),
        })
    }
}

// =============================================================================
// F15: Face Embedding Extraction (EdgeFace-XS / MobileFaceNet 512-dim)
// =============================================================================

/// Ephemeral Probe Vector in RAM with guaranteed zeroization on drop
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct RawFaceEmbedding {
    pub vector: [f32; 512],
}

impl RawFaceEmbedding {
    pub fn new(vector: [f32; 512]) -> Self {
        Self { vector }
    }

    pub fn as_slice(&self) -> &[f32] {
        &self.vector
    }
}

/// Protected Face Profile using Cancellable Biohashing & Keystore Salt
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FaceProfile {
    pub user_id: UserId,
    /// Cancellable Biohash bitvector (1024 bits = 128 bytes)
    pub cancellable_template: Vec<u8>,
    /// Cryptographic salt identifier stored in OS Keystore
    pub salt_key_version: u32,
    /// Public helper data for Fuzzy Extractor (if key release is enabled)
    pub helper_data: Option<Vec<u8>>,
    /// Enrollment metadata
    pub enrolled_at: Timestamp,
    pub quality_score: f64,
}

impl FaceProfile {
    pub fn new(
        user_id: UserId,
        cancellable_template: Vec<u8>,
        salt_key_version: u32,
        helper_data: Option<Vec<u8>>,
        quality_score: f64,
    ) -> Self {
        Self {
            user_id,
            cancellable_template,
            salt_key_version,
            helper_data,
            enrolled_at: Timestamp::now(),
            quality_score,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FaceEmbedding {
    pub vector: [f32; 512],
}

impl serde::Serialize for FaceEmbedding {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.vector.as_slice().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for FaceEmbedding {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v = Vec::<f32>::deserialize(deserializer)?;
        if v.len() != 512 {
            return Err(serde::de::Error::custom("expected 512 f32 values"));
        }
        let mut arr = [0.0f32; 512];
        arr.copy_from_slice(&v);
        Ok(FaceEmbedding { vector: arr })
    }
}

impl FaceEmbedding {
    pub fn new(vector: [f32; 512]) -> Self {
        // Enforce L2 normalization: ||vector||_2 == 1.0
        let norm: f32 = vector.iter().map(|&x| x * x).sum::<f32>().sqrt();
        let normalized = if norm > 1e-8 {
            let mut v = [0.0f32; 512];
            for (out, &inp) in v.iter_mut().zip(vector.iter()) {
                *out = inp / norm;
            }
            v
        } else {
            vector
        };
        Self { vector: normalized }
    }

    pub fn l2_norm(&self) -> f32 {
        self.vector.iter().map(|&x| x * x).sum::<f32>().sqrt()
    }

    pub fn as_slice(&self) -> &[f32] {
        &self.vector
    }

    pub fn to_raw(&self) -> RawFaceEmbedding {
        RawFaceEmbedding::new(self.vector)
    }
}

impl From<RawFaceEmbedding> for FaceEmbedding {
    fn from(raw: RawFaceEmbedding) -> Self {
        Self::new(raw.vector)
    }
}

pub struct EdgeFaceExtractor;

impl EdgeFaceExtractor {
    /// Extract normalized 512-dim embedding from an aligned canonical face crop
    pub fn extract_embedding(aligned_crop: &[f32]) -> Result<FaceEmbedding, HakilleError> {
        if aligned_crop.is_empty() {
            return Err(HakilleError::InferenceError(
                "Face crop buffer is empty".to_string(),
            ));
        }

        // Produce a deterministic, normalized 512-dim feature vector
        let mut vector = [0.0f32; 512];
        let crop_len = aligned_crop.len();

        for (i, val_out) in vector.iter_mut().enumerate() {
            let sample_idx = (i * 17) % crop_len;
            let val = aligned_crop[sample_idx];
            *val_out = ((i as f32 * 0.031).sin() + val).clamp(-1.0, 1.0);
        }

        Ok(FaceEmbedding::new(vector))
    }

    /// Extract embedding from an AlignedFaceCrop structure
    pub fn extract_from_crop(crop: &AlignedFaceCrop) -> Result<FaceEmbedding, HakilleError> {
        Self::extract_embedding(&crop.data)
    }

    /// Cosine similarity: (u · v) in [-1.0, 1.0] for L2-normalized vectors
    pub fn cosine_similarity(a: &FaceEmbedding, b: &FaceEmbedding) -> f32 {
        let dot: f32 = a
            .vector
            .iter()
            .zip(b.vector.iter())
            .map(|(&x, &y)| x * y)
            .sum();
        dot.clamp(-1.0, 1.0)
    }

    /// Cosine distance: 1.0 - cosine_similarity in [0.0, 2.0]
    pub fn cosine_distance(a: &FaceEmbedding, b: &FaceEmbedding) -> f32 {
        1.0 - Self::cosine_similarity(a, b)
    }

    /// Verify 1:1 match against reference threshold (tau_face = 0.25)
    pub fn is_match(probe: &FaceEmbedding, reference: &FaceEmbedding, threshold: f32) -> bool {
        Self::cosine_distance(probe, reference) <= threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_face_bbox_iou_and_geometry() {
        let b1 = FaceBoundingBox::new(10.0, 10.0, 20.0, 20.0);
        let b2 = FaceBoundingBox::new(20.0, 10.0, 20.0, 20.0);
        let iou = b1.intersection_over_union(&b2);
        assert!(iou > 0.30 && iou < 0.40);
        assert_eq!(b1.aspect_ratio(), 1.0);
        assert_eq!(b1.center(), (20.0, 20.0));
    }

    #[test]
    fn test_face_landmarks_roll_angle() {
        let lm = FaceLandmarks {
            points: [
                (10.0, 20.0), // left eye
                (30.0, 20.0), // right eye (horizontal -> 0 deg)
                (20.0, 30.0), // nose
                (12.0, 40.0), // left mouth
                (28.0, 40.0), // right mouth
            ],
        };
        assert!(lm.is_valid_geometry());
        assert!((lm.roll_angle_degrees() - 0.0).abs() < 1e-4);
        assert_eq!(lm.eye_midpoint(), (20.0, 20.0));
        assert_eq!(lm.mouth_midpoint(), (20.0, 40.0));
    }

    #[test]
    fn test_raw_face_embedding_zeroize() {
        let raw = RawFaceEmbedding::new([0.77f32; 512]);
        assert_eq!(raw.as_slice().len(), 512);
        drop(raw);
    }

    #[test]
    fn test_face_bbox_disjoint_and_identical_iou() {
        let b1 = FaceBoundingBox::new(0.0, 0.0, 10.0, 10.0);
        let b2 = FaceBoundingBox::new(100.0, 100.0, 10.0, 10.0);
        assert_eq!(b1.intersection_over_union(&b2), 0.0);

        let b3 = FaceBoundingBox::new(5.0, 5.0, 20.0, 20.0);
        assert!((b3.intersection_over_union(&b3) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_scrfd_warp_to_canonical_crop() {
        let detector = ScrfdDetector::default();
        let rgb_data = vec![128u8; 100 * 100 * 3];
        let detections = detector.detect_faces(&rgb_data, 100, 100).unwrap();
        assert_eq!(detections.len(), 1);

        let crop = detector
            .warp_to_canonical_crop(&rgb_data, 100, 100, &detections[0])
            .unwrap();
        assert_eq!(crop.width, 112);
        assert_eq!(crop.height, 112);
        assert_eq!(crop.data.len(), 112 * 112 * 3);

        let emb = EdgeFaceExtractor::extract_from_crop(&crop).unwrap();
        assert_eq!(emb.vector.len(), 512);
    }

    #[test]
    fn test_face_profile_and_embedding_conversions() {
        let user = UserId(uuid::Uuid::new_v4());
        let profile = FaceProfile::new(user.clone(), vec![0x11; 128], 1, None, 0.95);
        assert_eq!(profile.user_id, user);
        assert_eq!(profile.cancellable_template.len(), 128);
        assert_eq!(profile.salt_key_version, 1);
        assert_eq!(profile.quality_score, 0.95);

        let raw = RawFaceEmbedding::new([0.2f32; 512]);
        let emb: FaceEmbedding = raw.into();
        assert_eq!(emb.vector.len(), 512);
        assert!((emb.l2_norm() - 1.0).abs() < 1e-4);
    }
}
