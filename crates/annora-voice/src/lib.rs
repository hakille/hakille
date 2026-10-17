pub mod barge_in;
pub mod codec;
pub mod moshi;
pub mod tts;

pub use barge_in::*;
pub use codec::*;
pub use moshi::*;
pub use tts::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_barge_in_interruption_detection() {
        // Trigger after 2 consecutive speech frames with prob >= 0.5
        let mut detector = BargeInDetector::new(0.5, 2);

        // Case 1: System is silent, user speaks -> No barge in
        assert!(detector.check_user_audio(0.9, 0.5).is_none());

        // Case 2: System starts speaking
        detector.start_system_speech("turn-1", "Bonjour, voici votre synthèse du jour.");
        assert!(detector.is_system_speaking());

        // Single speech frame: not enough to trigger (prevents cough / noise glitches)
        assert!(detector.check_user_audio(0.8, 0.4).is_none());
        assert!(detector.is_system_speaking());

        // Second consecutive frame: barge-in triggers!
        detector.update_playback_time(0.16);
        let event = detector.check_user_audio(0.85, 0.45);
        assert!(event.is_some());
        let ev = event.unwrap();
        assert_eq!(ev.interrupted_turn_id, "turn-1");
        assert!(
            !detector.is_system_speaking(),
            "System speech must be immediately halted"
        );
    }

    #[test]
    fn test_mimi_codec_causal_latency_invariants() {
        let mut codec = MimiCodec::new();
        assert_eq!(MIMI_SAMPLE_RATE, 24_000);
        assert_eq!(MIMI_FRAME_RATE_HZ, 12.5);
        assert_eq!(MIMI_FRAME_DURATION_MS, 80);

        // Test frame encoding/decoding
        let input_samples = vec![0.1f32; SAMPLES_PER_FRAME];
        let tokens = codec
            .encode_frame(&input_samples)
            .expect("Encoding frame should succeed");
        assert_eq!(tokens.codebooks.len(), MIMI_NUM_CODEBOOKS);

        let decoded = codec
            .decode_frame(&tokens)
            .expect("Decoding frame should succeed");
        assert_eq!(decoded.len(), SAMPLES_PER_FRAME);
    }
}
