use hakille_core::{CapabilityAction, CapabilityAuthority, CapabilityScope, Timestamp};
use hakille_substrate::{CameraConfig, CameraFacing, ImageFormat, Substrate};
use hakille_substrate_linux::{LinuxSubstrate, bgr_to_rgb, camera_v4l2, yuyv_to_rgb8};
use std::time::Duration;

#[test]
fn test_adv_v4l2_enumerate_devices_headless_no_crash() {
    let devices = camera_v4l2::enumerate_devices();
    // In headless container, no /dev/video* exists.
    // The function must safely probe /dev/video0..63 and return empty vec without panic.
    println!("Enumerated devices count: {}", devices.len());
}

#[test]
fn test_adv_v4l2_select_device_empty_list_returns_none() {
    assert!(camera_v4l2::select_device(&[], CameraFacing::Front).is_none());
    assert!(camera_v4l2::select_device(&[], CameraFacing::Back).is_none());
    assert!(camera_v4l2::select_device(&[], CameraFacing::External).is_none());
}

#[test]
fn test_adv_v4l2_start_without_emulation_fails_honestly() {
    let _env_guard = camera_v4l2::CAMERA_EMULATION_ENV_LOCK.blocking_lock();
    unsafe { std::env::remove_var("HAKILLE_CAMERA_EMULATION") };
    let config = CameraConfig {
        width: 640,
        height: 480,
        fps: 30,
        facing: CameraFacing::Front,
    };
    match camera_v4l2::start(&config) {
        Ok(stream) => {
            stream.stop_and_join();
            assert!(
                !camera_v4l2::enumerate_devices().is_empty(),
                "Should have failed when /dev/video* does not exist"
            );
        }
        Err(e) => {
            assert!(
                e.contains("camera unavailable") && e.contains("V4L2"),
                "Error message must be explicit, got: {e}"
            );
        }
    }
}

#[test]
fn test_adv_v4l2_refuses_when_emulation_not_enabled() {
    let _env_guard = camera_v4l2::CAMERA_EMULATION_ENV_LOCK.blocking_lock();
    if !camera_v4l2::enumerate_devices().is_empty() {
        return;
    }
    for val in &["0", "false", "FALSE", "disabled", "junk", ""] {
        unsafe { std::env::set_var("HAKILLE_CAMERA_EMULATION", val) };
        let config = CameraConfig {
            width: 640,
            height: 480,
            fps: 30,
            facing: CameraFacing::Front,
        };
        assert!(
            camera_v4l2::start(&config).is_err(),
            "Emulation must not activate for value: {val}"
        );
    }
    unsafe { std::env::remove_var("HAKILLE_CAMERA_EMULATION") };
}

#[tokio::test]
async fn test_adv_v4l2_emulation_activations_and_lifecycle() {
    let _env_guard = camera_v4l2::CAMERA_EMULATION_ENV_LOCK.lock().await;
    for val in &["1", "true", "TRUE", "True"] {
        unsafe { std::env::set_var("HAKILLE_CAMERA_EMULATION", val) };
        let config = CameraConfig {
            width: 320,
            height: 240,
            fps: 30,
            facing: CameraFacing::Front,
        };

        let stream = camera_v4l2::start(&config).expect("Emulated stream start must succeed");
        let receiver = stream.latest();
        let frame1 = camera_v4l2::next_frame(receiver.clone(), Duration::from_secs(2))
            .await
            .expect("First frame received");
        assert_eq!(frame1.width, 320);
        assert_eq!(frame1.height, 240);
        assert_eq!(frame1.data.len(), 320 * 240 * 3);
        assert!(matches!(frame1.format, ImageFormat::Rgb8));

        let frame2 = camera_v4l2::next_frame(receiver, Duration::from_secs(2))
            .await
            .expect("Second frame received");
        assert!(
            frame2.timestamp_ms >= frame1.timestamp_ms,
            "Timestamps must be monotonically increasing"
        );

        stream.stop_and_join();
    }
    unsafe { std::env::remove_var("HAKILLE_CAMERA_EMULATION") };
}

#[tokio::test]
async fn test_adv_v4l2_emulation_zero_dimension_fallback() {
    let _env_guard = camera_v4l2::CAMERA_EMULATION_ENV_LOCK.lock().await;
    unsafe { std::env::set_var("HAKILLE_CAMERA_EMULATION", "1") };
    let config = CameraConfig {
        width: 0,
        height: 0,
        fps: 0,
        facing: CameraFacing::Front,
    };
    let stream = camera_v4l2::start(&config).expect("Start with 0x0 must fallback to defaults");
    let receiver = stream.latest();
    let frame = camera_v4l2::next_frame(receiver, Duration::from_secs(2))
        .await
        .expect("Frame received from fallback config");
    assert_eq!(frame.width, 640);
    assert_eq!(frame.height, 480);
    assert_eq!(frame.data.len(), 640 * 480 * 3);
    stream.stop_and_join();
    unsafe { std::env::remove_var("HAKILLE_CAMERA_EMULATION") };
}

#[test]
fn test_adv_yuyv_to_rgb8_underflow_checks() {
    // 1. Input truncated
    let truncated_in = vec![128u8; 100];
    let mut out = vec![0u8; 640 * 480 * 3];
    assert!(
        yuyv_to_rgb8(&truncated_in, 640, 480, &mut out).is_err(),
        "Truncated input must return Err"
    );

    // 2. Output truncated
    let valid_in = vec![128u8; 640 * 480 * 2];
    let mut truncated_out = vec![0u8; 100];
    assert!(
        yuyv_to_rgb8(&valid_in, 640, 480, &mut truncated_out).is_err(),
        "Truncated output must return Err"
    );

    // 3. Exact 2x1 conversion
    let yuyv_2px = [128u8, 128, 128, 128];
    let mut rgb_2px = [0u8; 6];
    assert!(yuyv_to_rgb8(&yuyv_2px, 2, 1, &mut rgb_2px).is_ok());
    assert_eq!(rgb_2px, [128, 128, 128, 128, 128, 128]);
}

#[test]
fn test_adv_bgr_to_rgb_exact() {
    let bgr = [10u8, 20, 30, 40, 50, 60];
    let mut rgb = [0u8; 6];
    bgr_to_rgb(&bgr, &mut rgb);
    assert_eq!(rgb, [30, 20, 10, 60, 50, 40]);
}

#[test]
fn test_adv_bgr_to_rgb_truncated_out_panic_behavior() {
    let bgr = [10u8, 20, 30, 40, 50, 60];
    let mut truncated_out = [0u8; 3]; // Only room for 1 pixel, but bgr has 2 pixels
    let panic_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        bgr_to_rgb(&bgr, &mut truncated_out);
    }));
    assert!(
        panic_res.is_err(),
        "Confirmed: bgr_to_rgb panics without bounds check when out.len() < bgr.len()"
    );
}

#[tokio::test]
async fn test_adv_substrate_camera_capability_denied() {
    let authority = CapabilityAuthority::new();
    let substrate = LinuxSubstrate::new().with_trusted_root(authority.root_public_key());
    let wrong_cap = authority
        .create_root_token(
            vec![CapabilityScope::Filesystem],
            vec![CapabilityAction::Read],
            Timestamp(chrono::Utc::now() + chrono::Duration::hours(1)),
        )
        .unwrap();

    let config = CameraConfig {
        width: 640,
        height: 480,
        fps: 30,
        facing: CameraFacing::Front,
    };

    let res = substrate.start_camera_capture(config, &wrong_cap).await;
    assert!(
        matches!(res, Err(hakille_core::HakilleError::CapabilityDenied(_))),
        "Must reject non-Camera capability before accessing hardware"
    );
}
