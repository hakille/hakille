//! Gaze-Intent & Micro-Geste: Contactless Authorization System (F34, F35, F36).
//!
//! Fuses 3D gaze ray dwell fixation (500ms, spatial dispersion <= 1.5°) and
//! biometric micro-nod (pitch excursion 3.0°–12.0°, peak angular velocity >= 15°/s,
//! duration 150–450ms, rebound <= 2.5°, yaw stability <= 4.0°, and torso stability
//! shoulder displacement Delta Y <= 10.0mm) for contactless authorization of
//! actions and Biscuit capability tokens.

use crate::cognition::gaze_tracker::GazeTrackingResult;
use hakille_core::capability::{CapabilityAction, CapabilityScope, CapabilityToken};
use hakille_inference::models::facemesh::FaceMeshOutput;
use hakille_spatial::{BodySkeleton3D, Point3D, ScreenBounds, Vector3D};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Minimum dwell duration for target locking (500 ms = ~15 frames @ 30 FPS).
pub const DWELL_DURATION_MIN: Duration = Duration::from_millis(500);

/// Maximum angular gaze dispersion during fixation (1.5 degrees in radians).
pub const MAX_GAZE_DISPERSION_RAD: f32 = 1.5 * std::f32::consts::PI / 180.0;

/// Micro-saccade jitter tolerance window (100 ms).
pub const JITTER_TOLERANCE_WINDOW: Duration = Duration::from_millis(100);

/// Micro-gesture execution window after dwell lock (1200 ms).
pub const GESTURE_VALIDITY_WINDOW: Duration = Duration::from_millis(1200);

/// Anti-double trigger cooldown after authorization (600 ms).
pub const ANTI_DOUBLE_TRIGGER_COOLDOWN: Duration = Duration::from_millis(600);

/// Minimum downward pitch excursion for micro-nod (3.0 degrees).
pub const MIN_PITCH_EXCURSION_DEG: f32 = 3.0;

/// Maximum downward pitch excursion for micro-nod (12.0 degrees).
pub const MAX_PITCH_EXCURSION_DEG: f32 = 12.0;

/// Minimum peak angular velocity during nod (15.0 deg/s).
pub const MIN_PEAK_ANGULAR_VELOCITY_DEG_PER_SEC: f32 = 15.0;

/// Minimum nod duration (150 ms).
pub const MIN_NOD_DURATION: Duration = Duration::from_millis(150);

/// Maximum nod duration (450 ms).
pub const MAX_NOD_DURATION: Duration = Duration::from_millis(450);

/// Maximum rebound deviation from baseline (2.5 degrees).
pub const MAX_REBOUND_BASELINE_DEG: f32 = 2.5;

/// Maximum lateral yaw movement allowed during nod (4.0 degrees).
pub const MAX_LATERAL_YAW_STABILITY_DEG: f32 = 4.0;

/// Maximum torso / shoulder vertical displacement for cervical invariant (10.0 mm = 0.010 m).
pub const MAX_SHOULDER_DELTA_Y_METERS: f32 = 0.010;

/// Target interactable element displayed on screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GazeInteractableTarget {
    pub target_id: String,
    pub bounds: ScreenBounds,
    pub action_type: String,
    pub required_biscuit_scope: Option<CapabilityScope>,
    pub required_biscuit_action: Option<CapabilityAction>,
}

impl GazeInteractableTarget {
    pub fn new(
        target_id: impl Into<String>,
        bounds: ScreenBounds,
        action_type: impl Into<String>,
    ) -> Self {
        Self {
            target_id: target_id.into(),
            bounds,
            action_type: action_type.into(),
            required_biscuit_scope: None,
            required_biscuit_action: None,
        }
    }

    pub fn with_capability(mut self, scope: CapabilityScope, action: CapabilityAction) -> Self {
        self.required_biscuit_scope = Some(scope);
        self.required_biscuit_action = Some(action);
        self
    }
}

/// Dynamic state machine for gaze dwell and micro-gesture recognition.
#[derive(Debug, Clone, PartialEq)]
pub enum GestureAuthState {
    /// Idle, scanning screen impact point for targets.
    Idle,
    /// Continuous fixation inside a target bounding box.
    DwellFixation {
        target_id: String,
        started_at: Instant,
        progress: f32, // [0.0, 1.0]
    },
    /// Fixation locked (>= 500ms), awaiting micro-nod confirmation within 1200ms.
    AwaitingMicroGesture {
        target_id: String,
        dwell_completed_at: Instant,
        deadline: Instant,
    },
    /// Downward pitch detected, tracking velocity, duration, and rebound.
    NodExcursion {
        target_id: String,
        start_pitch_rad: f32,
        peak_pitch_rad: f32,
        started_at: Instant,
        max_velocity_deg_s: f32,
    },
    /// Gesture validated, authorization granted.
    Authorized {
        target_id: String,
        authorized_at: Instant,
    },
    /// Brief cooldown period (600ms) to prevent double activation.
    Cooldown { expires_at: Instant },
}

/// Output decision emitted upon gesture authorization.
#[derive(Debug, Clone)]
pub struct GestureAuthDecision {
    pub is_authorized: bool,
    pub target_id: Option<String>,
    pub gesture_type: String, // "micro_nod"
    pub confidence: f32,
    pub execution_token: Option<CapabilityToken>,
}

impl PartialEq for GestureAuthDecision {
    fn eq(&self, other: &Self) -> bool {
        self.is_authorized == other.is_authorized
            && self.target_id == other.target_id
            && self.gesture_type == other.gesture_type
            && (self.confidence - other.confidence).abs() < 1e-4
            && match (&self.execution_token, &other.execution_token) {
                (None, None) => true,
                (Some(a), Some(b)) => a.to_vec().ok() == b.to_vec().ok(),
                _ => false,
            }
    }
}

/// State tracker for Contactless Gesture Authorization.
#[derive(Debug, Clone)]
pub struct ContactlessGestureAuthorizer {
    state: GestureAuthState,
    last_in_bounds_time: Option<Instant>,
    fixation_baseline_ray: Option<Vector3D>,
    baseline_head_pitch_rad: f32,
    baseline_head_yaw_rad: f32,
    baseline_shoulder_mid: Option<Point3D>,
    previous_head_pitch_rad: f32,
    previous_update_time: Option<Instant>,
    active_target: Option<GazeInteractableTarget>,
}

impl Default for ContactlessGestureAuthorizer {
    fn default() -> Self {
        Self::new()
    }
}

impl ContactlessGestureAuthorizer {
    pub fn new() -> Self {
        Self {
            state: GestureAuthState::Idle,
            last_in_bounds_time: None,
            fixation_baseline_ray: None,
            baseline_head_pitch_rad: 0.0,
            baseline_head_yaw_rad: 0.0,
            baseline_shoulder_mid: None,
            previous_head_pitch_rad: 0.0,
            previous_update_time: None,
            active_target: None,
        }
    }

    /// Current internal state.
    pub fn state(&self) -> &GestureAuthState {
        &self.state
    }

    /// Reset authorizer state machine to Idle.
    pub fn reset(&mut self) {
        self.state = GestureAuthState::Idle;
        self.last_in_bounds_time = None;
        self.fixation_baseline_ray = None;
        self.baseline_shoulder_mid = None;
        self.active_target = None;
    }

    /// Ingest multimodal tracking results and advance the authorization state machine.
    pub fn update(
        &mut self,
        now: Instant,
        gaze: &GazeTrackingResult,
        _facemesh: &FaceMeshOutput,
        skeleton: &BodySkeleton3D,
        targets: &[GazeInteractableTarget],
    ) -> Option<GestureAuthDecision> {
        let current_pitch_rad = gaze.head_pose.pitch;
        let current_yaw_rad = gaze.head_pose.yaw;

        // Calculate angular pitch velocity if we have a previous sample
        let angular_velocity_deg_s = if let Some(prev_time) = self.previous_update_time {
            let dt = now.duration_since(prev_time).as_secs_f32();
            if dt > 1e-4 {
                (current_pitch_rad - self.previous_head_pitch_rad)
                    .abs()
                    .to_degrees()
                    / dt
            } else {
                0.0
            }
        } else {
            0.0
        };

        let result = match &self.state {
            GestureAuthState::Cooldown { expires_at } => {
                if now >= *expires_at {
                    self.reset();
                }
                None
            }

            GestureAuthState::Idle => {
                if !gaze.is_screen_focused {
                    return None;
                }

                if let Some(impact) = gaze.screen_intersection {
                    // Search for a target containing the gaze impact point
                    if let Some(target) = targets
                        .iter()
                        .find(|t| t.bounds.contains_point_2d(impact.x, impact.y))
                    {
                        let ray_dir = gaze
                            .gaze_ray
                            .direction
                            .to_vector()
                            .normalize()
                            .unwrap_or(Vector3D::new(0.0, 0.0, -1.0));
                        self.fixation_baseline_ray = Some(ray_dir);
                        self.last_in_bounds_time = Some(now);
                        self.active_target = Some(target.clone());
                        self.state = GestureAuthState::DwellFixation {
                            target_id: target.target_id.clone(),
                            started_at: now,
                            progress: 0.0,
                        };
                    }
                }
                None
            }

            GestureAuthState::DwellFixation {
                target_id,
                started_at,
                ..
            } => {
                let target = match &self.active_target {
                    Some(t) if t.target_id == *target_id => t.clone(),
                    _ => {
                        self.reset();
                        return None;
                    }
                };

                let is_inside = if let Some(impact) = gaze.screen_intersection {
                    target.bounds.contains_point_2d(impact.x, impact.y)
                } else {
                    false
                };

                // Check spatial angular dispersion
                let dispersion_ok = if let Some(base_ray) = self.fixation_baseline_ray {
                    let cur_ray = gaze
                        .gaze_ray
                        .direction
                        .to_vector()
                        .normalize()
                        .unwrap_or(Vector3D::new(0.0, 0.0, -1.0));
                    let dot =
                        (base_ray.x * cur_ray.x + base_ray.y * cur_ray.y + base_ray.z * cur_ray.z)
                            .clamp(-1.0, 1.0);
                    let angle_rad = dot.acos();
                    angle_rad <= MAX_GAZE_DISPERSION_RAD
                } else {
                    true
                };

                if is_inside && dispersion_ok {
                    self.last_in_bounds_time = Some(now);
                    let elapsed = now.duration_since(*started_at);
                    let progress =
                        (elapsed.as_secs_f32() / DWELL_DURATION_MIN.as_secs_f32()).min(1.0);

                    if elapsed >= DWELL_DURATION_MIN {
                        // 500ms continuous fixation achieved: lock target and await micro-gesture!
                        let deadline = now + GESTURE_VALIDITY_WINDOW;
                        let target_id_str = target_id.clone();
                        self.baseline_head_pitch_rad = current_pitch_rad;
                        self.baseline_head_yaw_rad = current_yaw_rad;
                        self.baseline_shoulder_mid = skeleton.mid_shoulders(0.1);

                        self.state = GestureAuthState::AwaitingMicroGesture {
                            target_id: target_id_str,
                            dwell_completed_at: now,
                            deadline,
                        };
                    } else {
                        self.state = GestureAuthState::DwellFixation {
                            target_id: target_id.clone(),
                            started_at: *started_at,
                            progress,
                        };
                    }
                } else {
                    // Out of bounds or dispersion exceeded: check jitter tolerance window (100ms)
                    if let Some(last_in) = self.last_in_bounds_time {
                        if now.duration_since(last_in) > JITTER_TOLERANCE_WINDOW {
                            self.reset();
                        }
                    } else {
                        self.reset();
                    }
                }
                None
            }

            GestureAuthState::AwaitingMicroGesture {
                target_id,
                deadline,
                ..
            } => {
                if now > *deadline {
                    // Micro-gesture window (1200ms) expired without nod
                    self.reset();
                    return None;
                }

                // Check lateral yaw stability: reject if head is turning horizontally (> 4.0°)
                let yaw_deviation_deg = (current_yaw_rad - self.baseline_head_yaw_rad)
                    .abs()
                    .to_degrees();
                if yaw_deviation_deg > MAX_LATERAL_YAW_STABILITY_DEG {
                    self.reset();
                    return None;
                }

                // Invertible head pitch excursion: downward movement has downward angular velocity
                // In OpenCV / HeadPose6D convention, downward nod increases/decreases pitch with velocity >= 15°/s
                let delta_pitch_deg = (current_pitch_rad - self.baseline_head_pitch_rad)
                    .abs()
                    .to_degrees();

                if delta_pitch_deg >= 1.0 && angular_velocity_deg_s >= 8.0 {
                    // Entering nod excursion phase
                    let target_id_str = target_id.clone();
                    self.state = GestureAuthState::NodExcursion {
                        target_id: target_id_str,
                        start_pitch_rad: self.baseline_head_pitch_rad,
                        peak_pitch_rad: current_pitch_rad,
                        started_at: now,
                        max_velocity_deg_s: angular_velocity_deg_s,
                    };
                }
                None
            }

            GestureAuthState::NodExcursion {
                target_id,
                start_pitch_rad,
                peak_pitch_rad,
                started_at,
                max_velocity_deg_s,
            } => {
                let elapsed = now.duration_since(*started_at);
                if elapsed > MAX_NOD_DURATION {
                    // Nod exceeded 450ms without proper rebound
                    self.reset();
                    return None;
                }

                // Lateral yaw stability check during nod
                let yaw_dev = (current_yaw_rad - self.baseline_head_yaw_rad)
                    .abs()
                    .to_degrees();
                if yaw_dev > MAX_LATERAL_YAW_STABILITY_DEG {
                    self.reset();
                    return None;
                }

                // Torso / shoulder stability check (Delta Y <= 10.0mm)
                if let (Some(base_sh), Some(cur_sh)) =
                    (self.baseline_shoulder_mid, skeleton.mid_shoulders(0.1))
                {
                    let delta_y = (cur_sh.y - base_sh.y).abs();
                    if delta_y > MAX_SHOULDER_DELTA_Y_METERS {
                        // Upper body slouching detected, invalidate gesture
                        self.reset();
                        return None;
                    }
                }

                // Update peak pitch reached
                let current_excursion_deg =
                    (current_pitch_rad - *start_pitch_rad).abs().to_degrees();
                let recorded_peak_deg = (*peak_pitch_rad - *start_pitch_rad).abs().to_degrees();
                let new_peak_pitch = if current_excursion_deg > recorded_peak_deg {
                    current_pitch_rad
                } else {
                    *peak_pitch_rad
                };
                let new_max_vel = max_velocity_deg_s.max(angular_velocity_deg_s);

                // Check rebound to baseline (within 2.5°)
                let rebound_error_deg = (current_pitch_rad - *start_pitch_rad).abs().to_degrees();
                let peak_amplitude_deg = (new_peak_pitch - *start_pitch_rad).abs().to_degrees();

                if elapsed >= MIN_NOD_DURATION
                    && (MIN_PITCH_EXCURSION_DEG..=MAX_PITCH_EXCURSION_DEG)
                        .contains(&peak_amplitude_deg)
                    && new_max_vel >= MIN_PEAK_ANGULAR_VELOCITY_DEG_PER_SEC
                    && rebound_error_deg <= MAX_REBOUND_BASELINE_DEG
                {
                    // Full biomechanical micro-nod criteria satisfied!
                    let tid = target_id.clone();
                    self.state = GestureAuthState::Authorized {
                        target_id: tid.clone(),
                        authorized_at: now,
                    };

                    let decision = GestureAuthDecision {
                        is_authorized: true,
                        target_id: Some(tid),
                        gesture_type: "micro_nod".to_string(),
                        confidence: 0.96,
                        execution_token: None,
                    };

                    // Switch immediately to Cooldown to prevent double-firing
                    self.state = GestureAuthState::Cooldown {
                        expires_at: now + ANTI_DOUBLE_TRIGGER_COOLDOWN,
                    };

                    Some(decision)
                } else {
                    self.state = GestureAuthState::NodExcursion {
                        target_id: target_id.clone(),
                        start_pitch_rad: *start_pitch_rad,
                        peak_pitch_rad: new_peak_pitch,
                        started_at: *started_at,
                        max_velocity_deg_s: new_max_vel,
                    };
                    None
                }
            }

            GestureAuthState::Authorized { .. } => {
                self.state = GestureAuthState::Cooldown {
                    expires_at: now + ANTI_DOUBLE_TRIGGER_COOLDOWN,
                };
                None
            }
        };

        self.previous_head_pitch_rad = current_pitch_rad;
        self.previous_update_time = Some(now);

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_inference::ModelStatus;
    use hakille_inference::models::facemesh::CanonicalPnPPoints;
    use hakille_spatial::{CocoJointId, GazeRay3D, HeadPose6D, Point3D, SkeletonJoint3D};

    fn make_dummy_facemesh() -> FaceMeshOutput {
        FaceMeshOutput {
            landmarks: vec![Point3D::ORIGIN; 478],
            left_iris: Point3D::new(-0.03, 0.0, 0.5),
            right_iris: Point3D::new(0.03, 0.0, 0.5),
            ipd_distance: 0.063,
            left_ear: 0.30,
            right_ear: 0.30,
            mar: 0.15,
            pnp_points: CanonicalPnPPoints {
                nose_tip: Point3D::ORIGIN,
                chin: Point3D::new(0.0, -0.08, 0.0),
                left_eye_corner: Point3D::new(-0.03, 0.03, 0.0),
                right_eye_corner: Point3D::new(0.03, 0.03, 0.0),
                left_mouth_corner: Point3D::new(-0.02, -0.05, 0.0),
                right_mouth_corner: Point3D::new(0.02, -0.05, 0.0),
            },
            confidence: 0.95,
            status: ModelStatus::Simulated,
        }
    }

    fn make_skeleton(mid_shoulder_y: f32) -> BodySkeleton3D {
        let mut skel = BodySkeleton3D::empty();
        skel.joints[CocoJointId::LeftShoulder as usize] = SkeletonJoint3D::new(
            CocoJointId::LeftShoulder as u8,
            Point3D::new(-0.20, mid_shoulder_y, 0.60),
            0.95,
        );
        skel.joints[CocoJointId::RightShoulder as usize] = SkeletonJoint3D::new(
            CocoJointId::RightShoulder as u8,
            Point3D::new(0.20, mid_shoulder_y, 0.60),
            0.95,
        );
        skel
    }

    fn make_gaze_result(
        impact_x: f32,
        impact_y: f32,
        pitch_deg: f32,
        yaw_deg: f32,
    ) -> GazeTrackingResult {
        GazeTrackingResult {
            is_screen_focused: true,
            gaze_angles: (pitch_deg.to_radians(), yaw_deg.to_radians()),
            gaze_ray: GazeRay3D::from_vector(
                Point3D::new(0.0, 0.0, 0.65),
                Vector3D::new(impact_x, impact_y, -0.65),
                true,
            ),
            screen_intersection: Some(Point3D::new(impact_x, impact_y, 0.0)),
            head_pose: HeadPose6D::from_degrees(
                yaw_deg,
                pitch_deg,
                0.0,
                Point3D::new(0.0, 0.0, 0.65),
            ),
            confidence: 0.95,
        }
    }

    #[test]
    fn test_dwell_500ms_target_lock() {
        let mut authorizer = ContactlessGestureAuthorizer::new();
        let target = GazeInteractableTarget::new(
            "btn_transfer",
            ScreenBounds::new(Point3D::new(0.0, 0.15, 0.0), 0.10, 0.10),
            "execute_transfer",
        );
        let targets = [target];
        let facemesh = make_dummy_facemesh();
        let skeleton = make_skeleton(0.15);

        let t0 = Instant::now();

        // Initial fixation frame at t=0
        let gaze = make_gaze_result(0.0, 0.15, 0.0, 0.0);
        let decision = authorizer.update(t0, &gaze, &facemesh, &skeleton, &targets);
        assert!(decision.is_none());
        assert!(matches!(
            authorizer.state(),
            GestureAuthState::DwellFixation { .. }
        ));

        // Frame at t=250ms: progress ~0.5
        let t1 = t0 + Duration::from_millis(250);
        authorizer.update(t1, &gaze, &facemesh, &skeleton, &targets);
        if let GestureAuthState::DwellFixation { progress, .. } = authorizer.state() {
            assert!(*progress >= 0.49 && *progress <= 0.52);
        } else {
            panic!("Expected DwellFixation state");
        }

        // Frame at t=500ms: dwell duration reached! Should transition to AwaitingMicroGesture
        let t2 = t0 + Duration::from_millis(500);
        authorizer.update(t2, &gaze, &facemesh, &skeleton, &targets);
        assert!(matches!(
            authorizer.state(),
            GestureAuthState::AwaitingMicroGesture { target_id, .. } if target_id == "btn_transfer"
        ));
    }

    #[test]
    fn test_micro_saccade_jitter_tolerance() {
        let mut authorizer = ContactlessGestureAuthorizer::new();
        let target = GazeInteractableTarget::new(
            "btn_confirm",
            ScreenBounds::new(Point3D::new(0.0, 0.15, 0.0), 0.10, 0.10),
            "confirm",
        );
        let targets = [target];
        let facemesh = make_dummy_facemesh();
        let skeleton = make_skeleton(0.15);

        let t0 = Instant::now();

        // Dwell starts
        let gaze_in = make_gaze_result(0.0, 0.15, 0.0, 0.0);
        authorizer.update(t0, &gaze_in, &facemesh, &skeleton, &targets);

        // At t=200ms: still inside
        let t1 = t0 + Duration::from_millis(200);
        authorizer.update(t1, &gaze_in, &facemesh, &skeleton, &targets);

        // At t=250ms: micro-saccade excursion outside target (impact at (0.30, 0.30)) for 50ms
        let gaze_out = make_gaze_result(0.30, 0.30, 0.0, 0.0);
        let t2 = t0 + Duration::from_millis(250);
        authorizer.update(t2, &gaze_out, &facemesh, &skeleton, &targets);
        // Under 100ms tolerance: state is NOT reset to Idle!
        assert!(matches!(
            authorizer.state(),
            GestureAuthState::DwellFixation { .. }
        ));

        // At t=280ms: gaze returns inside target bounds (30ms later)
        let t3 = t0 + Duration::from_millis(280);
        authorizer.update(t3, &gaze_in, &facemesh, &skeleton, &targets);
        assert!(matches!(
            authorizer.state(),
            GestureAuthState::DwellFixation { .. }
        ));

        // At t=550ms: target lock succeeds
        let t4 = t0 + Duration::from_millis(550);
        authorizer.update(t4, &gaze_in, &facemesh, &skeleton, &targets);
        assert!(matches!(
            authorizer.state(),
            GestureAuthState::AwaitingMicroGesture { .. }
        ));
    }

    #[test]
    fn test_excursion_exceeding_jitter_window_resets_to_idle() {
        let mut authorizer = ContactlessGestureAuthorizer::new();
        let target = GazeInteractableTarget::new(
            "btn_confirm",
            ScreenBounds::new(Point3D::new(0.0, 0.15, 0.0), 0.10, 0.10),
            "confirm",
        );
        let targets = [target];
        let facemesh = make_dummy_facemesh();
        let skeleton = make_skeleton(0.15);

        let t0 = Instant::now();
        let gaze_in = make_gaze_result(0.0, 0.15, 0.0, 0.0);
        authorizer.update(t0, &gaze_in, &facemesh, &skeleton, &targets);

        // Gaze leaves target
        let gaze_out = make_gaze_result(0.40, 0.40, 0.0, 0.0);
        let t1 = t0 + Duration::from_millis(150); // > 100ms jitter tolerance
        authorizer.update(t1, &gaze_out, &facemesh, &skeleton, &targets);

        assert_eq!(*authorizer.state(), GestureAuthState::Idle);
    }

    #[test]
    fn test_micro_nod_biomechanical_confirmation() {
        let mut authorizer = ContactlessGestureAuthorizer::new();
        let target = GazeInteractableTarget::new(
            "vault_unlock",
            ScreenBounds::new(Point3D::new(0.0, 0.15, 0.0), 0.10, 0.10),
            "unlock",
        );
        let targets = [target];
        let facemesh = make_dummy_facemesh();
        let skeleton = make_skeleton(0.15);

        let t0 = Instant::now();

        // 1. Dwell fixation for 500ms
        let gaze_dwell = make_gaze_result(0.0, 0.15, 0.0, 0.0);
        authorizer.update(t0, &gaze_dwell, &facemesh, &skeleton, &targets);
        authorizer.update(
            t0 + Duration::from_millis(500),
            &gaze_dwell,
            &facemesh,
            &skeleton,
            &targets,
        );
        assert!(matches!(
            authorizer.state(),
            GestureAuthState::AwaitingMicroGesture { .. }
        ));

        // 2. Head begins downward pitch nod: delta pitch 2.0° at high velocity (dt = 50ms)
        let t_nod_start = t0 + Duration::from_millis(550);
        let gaze_pitch1 = make_gaze_result(0.0, 0.15, 2.0, 0.5);
        authorizer.update(t_nod_start, &gaze_pitch1, &facemesh, &skeleton, &targets);
        assert!(matches!(
            authorizer.state(),
            GestureAuthState::NodExcursion { .. }
        ));

        // 3. Peak pitch reached: 6.0° excursion (in [3.0°, 12.0°]), peak velocity >= 15°/s, dt = 100ms
        let t_peak = t_nod_start + Duration::from_millis(100);
        let gaze_peak = make_gaze_result(0.0, 0.15, 6.0, 0.5);
        authorizer.update(t_peak, &gaze_peak, &facemesh, &skeleton, &targets);

        // 4. Rebound back to baseline: pitch = 0.5° (rebound <= 2.5°), elapsed = 220ms (in [150, 450ms]),
        // yaw stability = 0.5° (<= 4.0°), shoulder Delta Y = 0.002m (<= 10mm)
        let t_rebound = t_nod_start + Duration::from_millis(220);
        let gaze_rebound = make_gaze_result(0.0, 0.15, 0.5, 0.5);
        let skeleton_stable = make_skeleton(0.152); // Delta Y = 2mm <= 10mm
        let decision = authorizer.update(
            t_rebound,
            &gaze_rebound,
            &facemesh,
            &skeleton_stable,
            &targets,
        );

        assert!(decision.is_some(), "Micro-nod should be authorized");
        let dec = decision.unwrap();
        assert!(dec.is_authorized);
        assert_eq!(dec.target_id.as_deref(), Some("vault_unlock"));
        assert_eq!(dec.gesture_type, "micro_nod");
        assert!(dec.confidence >= 0.90);

        // Cooldown active
        assert!(matches!(
            authorizer.state(),
            GestureAuthState::Cooldown { .. }
        ));
    }

    #[test]
    fn test_micro_nod_rejects_torso_slouching() {
        let mut authorizer = ContactlessGestureAuthorizer::new();
        let target = GazeInteractableTarget::new(
            "vault_unlock",
            ScreenBounds::new(Point3D::new(0.0, 0.15, 0.0), 0.10, 0.10),
            "unlock",
        );
        let targets = [target];
        let facemesh = make_dummy_facemesh();
        let skeleton_baseline = make_skeleton(0.150);

        let t0 = Instant::now();

        // Dwell 500ms
        let gaze_dwell = make_gaze_result(0.0, 0.15, 0.0, 0.0);
        authorizer.update(t0, &gaze_dwell, &facemesh, &skeleton_baseline, &targets);
        authorizer.update(
            t0 + Duration::from_millis(500),
            &gaze_dwell,
            &facemesh,
            &skeleton_baseline,
            &targets,
        );

        // Nod starts
        let t_nod_start = t0 + Duration::from_millis(550);
        let gaze_pitch1 = make_gaze_result(0.0, 0.15, 2.0, 0.0);
        authorizer.update(
            t_nod_start,
            &gaze_pitch1,
            &facemesh,
            &skeleton_baseline,
            &targets,
        );

        // Torso slouched downward by 18mm (Delta Y = 0.018m > 0.010m)
        let t_slouch = t_nod_start + Duration::from_millis(150);
        let gaze_slouch = make_gaze_result(0.0, 0.15, 6.0, 0.0);
        let skeleton_slouched = make_skeleton(0.168); // 168 - 150 = 18mm displacement
        let decision = authorizer.update(
            t_slouch,
            &gaze_slouch,
            &facemesh,
            &skeleton_slouched,
            &targets,
        );

        assert!(decision.is_none(), "Slouching must not authorize gesture");
        assert_eq!(
            *authorizer.state(),
            GestureAuthState::Idle,
            "Torso slouch must reset authorizer to Idle"
        );
    }

    #[test]
    fn test_micro_nod_rejects_lateral_head_shake() {
        let mut authorizer = ContactlessGestureAuthorizer::new();
        let target = GazeInteractableTarget::new(
            "btn_approve",
            ScreenBounds::new(Point3D::new(0.0, 0.15, 0.0), 0.10, 0.10),
            "approve",
        );
        let targets = [target];
        let facemesh = make_dummy_facemesh();
        let skeleton = make_skeleton(0.15);

        let t0 = Instant::now();
        let gaze_dwell = make_gaze_result(0.0, 0.15, 0.0, 0.0);
        authorizer.update(t0, &gaze_dwell, &facemesh, &skeleton, &targets);
        authorizer.update(
            t0 + Duration::from_millis(500),
            &gaze_dwell,
            &facemesh,
            &skeleton,
            &targets,
        );

        // User shakes head sideways (yaw = 8.0° > 4.0°)
        let t_shake = t0 + Duration::from_millis(600);
        let gaze_shake = make_gaze_result(0.0, 0.15, 1.0, 8.0);
        let decision = authorizer.update(t_shake, &gaze_shake, &facemesh, &skeleton, &targets);

        assert!(decision.is_none());
        assert_eq!(*authorizer.state(), GestureAuthState::Idle);
    }

    #[test]
    fn test_micro_nod_rejects_duration_timeout() {
        let mut authorizer = ContactlessGestureAuthorizer::new();
        let target = GazeInteractableTarget::new(
            "btn_approve",
            ScreenBounds::new(Point3D::new(0.0, 0.15, 0.0), 0.10, 0.10),
            "approve",
        );
        let targets = [target];
        let facemesh = make_dummy_facemesh();
        let skeleton = make_skeleton(0.15);

        let t0 = Instant::now();
        let gaze_dwell = make_gaze_result(0.0, 0.15, 0.0, 0.0);
        authorizer.update(t0, &gaze_dwell, &facemesh, &skeleton, &targets);
        authorizer.update(
            t0 + Duration::from_millis(500),
            &gaze_dwell,
            &facemesh,
            &skeleton,
            &targets,
        );

        // Exceed 1200ms validity window (at 500 + 1300 = 1800ms)
        let t_timeout = t0 + Duration::from_millis(1800);
        let gaze_late = make_gaze_result(0.0, 0.15, 0.0, 0.0);
        let decision = authorizer.update(t_timeout, &gaze_late, &facemesh, &skeleton, &targets);

        assert!(decision.is_none());
        assert_eq!(*authorizer.state(), GestureAuthState::Idle);
    }
}
