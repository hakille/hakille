//! Continuous Multimodal Fusion & Confidence Decay Gating (F20 & F21).
//! Real-time Bayesian/Kalman combination of face, voice, and proximity weak signals into
//! unified session confidence tau, with exponential temporal decay and step-up privilege escalation gating.

use hakille_core::capability::CapabilityScope;
use hakille_core::HakilleError;
use serde::{Deserialize, Serialize};

// =============================================================================
// F20: Continuous Multimodal Fusion Engine
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignalModality {
    FaceRecognition,
    VoicePrint,
    ProximityBle,
    KeystrokeDynamics,
}

impl SignalModality {
    /// Default empirical measurement noise variance R_k for Kalman filtering.
    /// Lower variance corresponds to higher sensor trust/reliability.
    pub fn default_variance(&self) -> f32 {
        match self {
            SignalModality::FaceRecognition => 0.04,   // High precision
            SignalModality::VoicePrint => 0.08,        // Medium-high precision
            SignalModality::KeystrokeDynamics => 0.15, // Medium precision
            SignalModality::ProximityBle => 0.25,      // Weak / ambient proximity signal
        }
    }

    /// Default baseline weight for weighted fusion
    pub fn default_weight(&self) -> f32 {
        match self {
            SignalModality::FaceRecognition => 0.50,
            SignalModality::VoicePrint => 0.30,
            SignalModality::KeystrokeDynamics => 0.10,
            SignalModality::ProximityBle => 0.10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultimodalSignal {
    pub modality: SignalModality,
    pub score: f32,  // in [0.0, 1.0]
    pub weight: f32, // e.g. Face: 0.50, Voice: 0.35, Proximity: 0.15
    pub timestamp: i64,
}

/// Real-time 1D Kalman filter tracking unified continuous session confidence tau in [0.0, 1.0].
/// Models confidence state x_t and uncertainty/covariance P_t over continuous time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KalmanConfidenceTracker {
    pub estimate: f32,         // Current state estimate tau in [0.0, 1.0]
    pub error_covariance: f32, // Estimation error variance P
    pub process_noise_q: f32,  // Uncertainty growth rate per second
    pub half_life_seconds: f32,// Half-life for temporal exponential decay
}

impl Default for KalmanConfidenceTracker {
    fn default() -> Self {
        Self {
            estimate: 0.50,
            error_covariance: 0.10,
            process_noise_q: 0.001,
            half_life_seconds: 180.0,
        }
    }
}

impl KalmanConfidenceTracker {
    pub fn new(initial_estimate: f32, initial_p: f32, process_noise_q: f32, half_life_seconds: f32) -> Self {
        Self {
            estimate: initial_estimate.clamp(0.0, 1.0),
            error_covariance: initial_p.max(1e-6),
            process_noise_q: process_noise_q.max(0.0),
            half_life_seconds: half_life_seconds.max(1.0),
        }
    }

    /// Decay rate lambda = ln(2) / half_life
    pub fn lambda(&self) -> f32 {
        0.69314718 / self.half_life_seconds
    }

    /// Predict step (time update): models exponential confidence decay and uncertainty diffusion.
    pub fn predict(&mut self, dt_seconds: f32) {
        if dt_seconds <= 0.0 {
            return;
        }
        // Fail-closed after 15 min (900 seconds)
        if dt_seconds >= 900.0 {
            self.estimate = 0.0;
            self.error_covariance = 1.0;
            return;
        }

        let decay_factor = (-self.lambda() * dt_seconds).exp();
        self.estimate = (self.estimate * decay_factor).clamp(0.0, 1.0);
        self.error_covariance += self.process_noise_q * dt_seconds;
    }

    /// Correct step (measurement update): fuses new observation z_k with variance R_k.
    pub fn update(&mut self, observation: f32, measurement_variance_r: f32) -> f32 {
        let z = observation.clamp(0.0, 1.0);
        let r = measurement_variance_r.max(1e-4);

        // Innovation y_k = z_k - x_{k|k-1}
        let innovation = z - self.estimate;
        // Innovation covariance S_k = P + R
        let s = self.error_covariance + r;
        // Kalman gain K_k = P / S_k
        let k = self.error_covariance / s;

        // State update x_{k|k} = x_{k|k-1} + K_k * y_k
        self.estimate = (self.estimate + k * innovation).clamp(0.0, 1.0);
        // Covariance update P_{k|k} = (1 - K_k) * P_{k|k-1}
        self.error_covariance = ((1.0 - k) * self.error_covariance).max(1e-6);

        self.estimate
    }

    /// Predict over elapsed time dt then sequentially fuse all incoming multimodal signals.
    pub fn step_and_fuse(&mut self, signals: &[MultimodalSignal], dt_seconds: f32) -> f32 {
        self.predict(dt_seconds);
        for s in signals {
            let r = s.modality.default_variance();
            self.update(s.score, r);
        }
        self.estimate
    }
}

/// Bayesian log-odds combiner for independent biometric and behavioural modalities.
pub struct BayesianMultimodalCombiner;

impl BayesianMultimodalCombiner {
    /// Combines prior confidence tau with multimodal evidence signals using Bayesian log-odds update.
    pub fn combine_log_odds(prior_tau: f32, signals: &[MultimodalSignal]) -> f32 {
        if signals.is_empty() {
            return prior_tau.clamp(0.0, 1.0);
        }

        // Bound prior to prevent infinite log-odds
        let clamped_prior = prior_tau.clamp(0.001, 0.999);
        let mut log_odds = (clamped_prior / (1.0 - clamped_prior)).ln();

        for s in signals {
            let score = s.score.clamp(0.01, 0.99);
            let weight = s.weight.clamp(0.0, 2.0);
            // Likelihood ratio LR = P(signal | genuine) / P(signal | impostor)
            let log_likelihood_ratio = (score / (1.0 - score)).ln();
            log_odds += weight * log_likelihood_ratio;
        }

        // Posterior confidence tau = 1 / (1 + exp(-log_odds))
        let posterior = 1.0 / (1.0 + (-log_odds).exp());
        posterior.clamp(0.0, 1.0)
    }
}

pub struct ContinuousMultimodalFusion {
    pub current_confidence: f32,
    pub kalman_tracker: KalmanConfidenceTracker,
}

impl Default for ContinuousMultimodalFusion {
    fn default() -> Self {
        Self {
            current_confidence: 0.50,
            kalman_tracker: KalmanConfidenceTracker::default(),
        }
    }
}

impl ContinuousMultimodalFusion {
    pub fn new(initial_confidence: f32) -> Self {
        let clamped = initial_confidence.clamp(0.0, 1.0);
        Self {
            current_confidence: clamped,
            kalman_tracker: KalmanConfidenceTracker::new(clamped, 0.10, 0.001, 180.0),
        }
    }

    /// Fuse multimodal weak signals into a unified confidence score tau in [0.0, 1.0]
    /// Preserves exact temporal smoothing formulation (60% new observation + 40% history).
    pub fn fuse_signals(&mut self, signals: &[MultimodalSignal]) -> f32 {
        if signals.is_empty() {
            return self.current_confidence;
        }

        let mut weighted_sum = 0.0f32;
        let mut total_weight = 0.0f32;

        for s in signals {
            let clamped_score = s.score.clamp(0.0, 1.0);
            weighted_sum += clamped_score * s.weight;
            total_weight += s.weight;
        }

        if total_weight > 0.0 {
            let fused_observation = weighted_sum / total_weight;
            // Adaptive smoothing: 60% new observation + 40% history
            self.current_confidence = (0.6 * fused_observation + 0.4 * self.current_confidence).clamp(0.0, 1.0);
        }

        // Synchronize Kalman tracker with fused confidence
        self.kalman_tracker.estimate = self.current_confidence;
        self.current_confidence
    }

    /// Alternative Bayesian log-odds update method for rigorous probabilistic fusion
    pub fn fuse_bayesian(&mut self, signals: &[MultimodalSignal]) -> f32 {
        self.current_confidence = BayesianMultimodalCombiner::combine_log_odds(self.current_confidence, signals);
        self.kalman_tracker.estimate = self.current_confidence;
        self.current_confidence
    }

    /// Real-time Kalman fusion step applying continuous prediction and sensor updates
    pub fn fuse_kalman(&mut self, signals: &[MultimodalSignal], dt_seconds: f32) -> f32 {
        self.current_confidence = self.kalman_tracker.step_and_fuse(signals, dt_seconds);
        self.current_confidence
    }
}

// =============================================================================
// F21: Confidence Decay & Privilege Escalation Gating
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepUpRequirement {
    None,
    ActiveBiometricChallenge,
    FaceLivenessVerification,
    HardwareSecurityKey,
}

pub struct ConfidenceDecayEngine {
    pub half_life_seconds: f32, // e.g. 180 seconds
}

impl Default for ConfidenceDecayEngine {
    fn default() -> Self {
        Self {
            half_life_seconds: 180.0,
        }
    }
}

impl ConfidenceDecayEngine {
    pub fn new(half_life_seconds: f32) -> Self {
        Self { half_life_seconds: half_life_seconds.max(1.0) }
    }

    /// Decay constant lambda = ln(2) / half_life
    pub fn decay_rate(&self) -> f32 {
        0.69314718 / self.half_life_seconds
    }

    /// Calculate temporal confidence decay: tau(t) = tau_0 * exp(-lambda * t) = tau_0 * (0.5)^(elapsed / half_life)
    pub fn apply_decay(&self, initial_tau: f32, elapsed_seconds: i64) -> f32 {
        if elapsed_seconds <= 0 {
            return initial_tau;
        }
        // Fail-closed timeout after 15 minutes (900 seconds) of zero fresh presence
        if elapsed_seconds >= 900 {
            return 0.0;
        }

        let lambda = self.decay_rate();
        let decayed = initial_tau * (-lambda * (elapsed_seconds as f32)).exp();
        decayed.clamp(0.0, 1.0)
    }

    /// Minimum confidence threshold required for a given capability scope
    pub fn required_threshold_for_scope(scope: &CapabilityScope) -> f32 {
        match scope {
            CapabilityScope::Wallet => 0.90,
            CapabilityScope::FullSystem => 0.90,
            CapabilityScope::ModelInference => 0.70,
            CapabilityScope::Filesystem => 0.50,
            CapabilityScope::Audio => 0.40,
            CapabilityScope::Camera => 0.40,
            CapabilityScope::Sensor => 0.30,
            CapabilityScope::Network => 0.40,
            CapabilityScope::GuiInput => 0.40,
            CapabilityScope::Biometrics => 0.70,
            CapabilityScope::Spatial => 0.40,
            CapabilityScope::CognitiveInference => 0.70,
            CapabilityScope::Notification => 0.50,
        }
    }

    /// Check if privilege step-up authentication is needed for target scope
    pub fn step_up_required(&self, current_tau: f32, scope: &CapabilityScope) -> bool {
        current_tau < Self::required_threshold_for_scope(scope)
    }

    /// Returns the required authentication step-up mechanism if current confidence is insufficient
    pub fn evaluate_step_up(&self, current_tau: f32, scope: &CapabilityScope) -> StepUpRequirement {
        let req = Self::required_threshold_for_scope(scope);
        if current_tau >= req {
            return StepUpRequirement::None;
        }

        match scope {
            CapabilityScope::Wallet | CapabilityScope::FullSystem => {
                StepUpRequirement::ActiveBiometricChallenge
            }
            CapabilityScope::ModelInference | CapabilityScope::Filesystem => {
                StepUpRequirement::FaceLivenessVerification
            }
            _ => StepUpRequirement::FaceLivenessVerification,
        }
    }

    /// Enforce admin elevation requirement (tau >= 0.90)
    pub fn admin_elevation_check(&self, current_tau: f32) -> Result<(), HakilleError> {
        self.gate_action(current_tau, &CapabilityScope::FullSystem)
    }

    /// Enforce crypto-wallet signing requirement (tau >= 0.90)
    pub fn wallet_signing_check(&self, current_tau: f32) -> Result<(), HakilleError> {
        self.gate_action(current_tau, &CapabilityScope::Wallet)
    }

    /// Gate operation: verifies if current tau satisfies privilege requirement
    pub fn gate_action(
        &self,
        current_tau: f32,
        scope: &CapabilityScope,
    ) -> Result<(), HakilleError> {
        let req = Self::required_threshold_for_scope(scope);
        if current_tau < req {
            return Err(HakilleError::CapabilityDenied(format!(
                "Privilege step-up required: current confidence {:.2} is below required threshold {:.2} for scope {:?}",
                current_tau, req, scope
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kalman_confidence_tracker_predict_and_update() {
        let mut tracker = KalmanConfidenceTracker::new(0.50, 0.10, 0.001, 180.0);

        // After 180 seconds without observation, estimate decays by ~50%
        tracker.predict(180.0);
        assert!((tracker.estimate - 0.25).abs() < 0.03);
        assert!(tracker.error_covariance > 0.10);

        // Strong face observation (0.95, low noise) pulls estimate up sharply
        let updated = tracker.update(0.95, SignalModality::FaceRecognition.default_variance());
        assert!(updated > 0.60);
        assert!(tracker.error_covariance < 0.10); // Measurement decreases uncertainty
    }

    #[test]
    fn test_bayesian_log_odds_combiner() {
        let signals = vec![
            MultimodalSignal {
                modality: SignalModality::FaceRecognition,
                score: 0.90,
                weight: 1.0,
                timestamp: 100,
            },
            MultimodalSignal {
                modality: SignalModality::VoicePrint,
                score: 0.85,
                weight: 1.0,
                timestamp: 100,
            },
        ];

        let post = BayesianMultimodalCombiner::combine_log_odds(0.50, &signals);
        assert!(post > 0.95, "Concordant strong face and voice yield very high posterior");
    }

    #[test]
    fn test_admin_and_wallet_elevation_checks() {
        let decay = ConfidenceDecayEngine::default();
        assert!(decay.wallet_signing_check(0.92).is_ok());
        assert!(decay.wallet_signing_check(0.88).is_err());
        assert!(decay.admin_elevation_check(0.95).is_ok());
        assert!(decay.admin_elevation_check(0.75).is_err());
    }

    #[test]
    fn test_step_up_evaluation() {
        let decay = ConfidenceDecayEngine::default();
        assert_eq!(
            decay.evaluate_step_up(0.60, &CapabilityScope::Wallet),
            StepUpRequirement::ActiveBiometricChallenge
        );
        assert_eq!(
            decay.evaluate_step_up(0.95, &CapabilityScope::Wallet),
            StepUpRequirement::None
        );
    }
}

