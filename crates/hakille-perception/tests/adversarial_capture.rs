use std::sync::Arc;
use sha2::{Digest, Sha256};
use hakille_perception::vision::capture::{
    bgra_to_rgb24, decode_mjpeg_to_rgb24, rgba_to_rgb24, yuyv_to_rgb24,
    FrameBufferPool, FrameFormat, VideoFrameBridge, VideoIngestionError,
};
use hakille_substrate::{CameraFrame, ImageFormat};

#[test]
fn test_adv_bridge_zero_dimensions_rejected_all_formats() {
    let pool = Arc::new(FrameBufferPool::new(2, 512));
    let mut bridge = VideoFrameBridge::with_pool(pool);

    let test_dimensions = [
        (0, 0, vec![1u8; 100]),
        (0, 64, vec![1u8; 100]),
        (64, 0, vec![1u8; 100]),
        (64, 64, vec![]),
    ];

    let formats = [
        FrameFormat::Rgb8,
        FrameFormat::Rgba8,
        FrameFormat::Bgra8,
        FrameFormat::Yuyv,
        FrameFormat::Jpeg,
    ];

    for (w, h, data) in test_dimensions {
        for fmt in &formats {
            let res = bridge.ingest_raw(&data, w, h, *fmt, 0);
            assert!(
                matches!(res, Err(VideoIngestionError::EmptyFrame { .. })),
                "Must return EmptyFrame for {}x{}, data_len={}, format={:?}",
                w, h, data.len(), fmt
            );
        }
    }
}

#[test]
fn test_adv_bridge_truncated_buffers_rejected_without_panic() {
    let pool = Arc::new(FrameBufferPool::new(2, 512));
    let mut bridge = VideoFrameBridge::with_pool(pool);

    let (w, h) = (8, 8); // 64 pixels
    // Expected sizes: RGB=192, RGBA/BGRA=256, YUYV=128

    // Test with 1 byte, 10 bytes, and expected-1 bytes
    for fmt in &[FrameFormat::Rgb8, FrameFormat::Rgba8, FrameFormat::Bgra8, FrameFormat::Yuyv] {
        let expected = match fmt {
            FrameFormat::Rgb8 => 64 * 3,
            FrameFormat::Rgba8 | FrameFormat::Bgra8 => 64 * 4,
            FrameFormat::Yuyv => 64 * 2,
            _ => unreachable!(),
        };

        for &actual_len in &[1, 10, expected - 1] {
            let truncated = vec![42u8; actual_len];
            let res = bridge.ingest_raw(&truncated, w, h, *fmt, 0);
            match res {
                Err(VideoIngestionError::BufferUnderflow { expected: exp, actual: act, format, .. }) => {
                    assert_eq!(exp, expected);
                    assert_eq!(act, actual_len);
                    assert_eq!(format, *fmt);
                }
                Err(e) => panic!("Expected BufferUnderflow for {:?} with len {}, got {:?}", fmt, actual_len, e),
                Ok(_) => panic!("Expected BufferUnderflow, but got Ok"),
            }
        }
    }
}

#[test]
fn test_adv_bridge_corrupted_jpeg_rejected_without_panic() {
    let pool = Arc::new(FrameBufferPool::new(2, 512));
    let mut bridge = VideoFrameBridge::with_pool(pool);

    let corrupt_payloads: Vec<Vec<u8>> = vec![
        vec![0xFF],
        vec![0xFF, 0xD8], // Only SOI header
        vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x02], // Truncated APP0
        vec![0x00; 100], // All zeros
        vec![0xAA; 256], // Random noise
        vec![0x7F, 0x45, 0x4C, 0x46], // ELF magic
    ];

    for (idx, payload) in corrupt_payloads.iter().enumerate() {
        let res = bridge.ingest_raw(payload, 64, 64, FrameFormat::Jpeg, 0);
        assert!(
            matches!(res, Err(VideoIngestionError::JpegDecodeError(_))),
            "Corrupt payload #{} must return JpegDecodeError", idx
        );
    }
}

#[test]
fn test_adv_bridge_extreme_dimensions_overflow_behavior() {
    let pool = Arc::new(FrameBufferPool::new(2, 512));
    let mut bridge = VideoFrameBridge::with_pool(pool);

    let dummy = vec![1u8; 100];

    // 1. Rgb8 uses checked_mul(3) properly: returns EmptyFrame on u32::MAX without panic
    let res_rgb = bridge.ingest_raw(&dummy, u32::MAX, u32::MAX, FrameFormat::Rgb8, 0);
    assert!(matches!(res_rgb, Err(VideoIngestionError::EmptyFrame { .. })));

    // 2. Large bounded dimensions (100_000 x 100_000): num_px fits in usize, buffer is truncated
    let res_bounded_rgba = bridge.ingest_raw(&dummy, 100_000, 100_000, FrameFormat::Rgba8, 0);
    assert!(matches!(res_bounded_rgba, Err(VideoIngestionError::BufferUnderflow { .. })));

    let res_bounded_yuyv = bridge.ingest_raw(&dummy, 100_000, 100_000, FrameFormat::Yuyv, 0);
    assert!(matches!(res_bounded_yuyv, Err(VideoIngestionError::BufferUnderflow { .. })));

    // 3. Documented Challenger Finding:
    // When width=u32::MAX and height=u32::MAX, (width * height) fits in usize on 64-bit,
    // but line 517 (num_px * 4) and line 541 (num_px * 4) and line 565 (num_px * 2)
    // use unchecked multiplication, causing an arithmetic overflow panic.
    let panic_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let pool = Arc::new(FrameBufferPool::new(2, 512));
        let mut bridge = VideoFrameBridge::with_pool(pool);
        let _ = bridge.ingest_raw(&dummy, u32::MAX, u32::MAX, FrameFormat::Rgba8, 0);
    }));
    assert!(panic_res.is_err(), "Confirmed: u32::MAX x u32::MAX triggers arithmetic overflow panic on num_px * 4");
}

#[test]
fn test_adv_conversions_yuyv_rgba_bgra_mjpeg() {
    // 1. yuyv_to_rgb24 math on 2 pixels
    let yuyv = [128u8, 128, 128, 128];
    let mut rgb = [0u8; 6];
    yuyv_to_rgb24(&yuyv, &mut rgb, 2, 1).expect("yuyv conversion ok");
    assert_eq!(rgb, [128, 128, 128, 128, 128, 128]);

    // 2. rgba_to_rgb24
    let rgba = [10u8, 20, 30, 255, 40, 50, 60, 255];
    let mut rgb2 = [0u8; 6];
    rgba_to_rgb24(&rgba, &mut rgb2, 2, 1).expect("rgba conversion ok");
    assert_eq!(rgb2, [10, 20, 30, 40, 50, 60]);

    // 3. bgra_to_rgb24
    let bgra = [30u8, 20, 10, 255, 60, 50, 40, 255];
    let mut rgb3 = [0u8; 6];
    bgra_to_rgb24(&bgra, &mut rgb3, 2, 1).expect("bgra conversion ok");
    assert_eq!(rgb3, [10, 20, 30, 40, 50, 60]);

    // 4. decode_mjpeg_to_rgb24 on empty
    let mut mjpeg_out = Vec::new();
    assert!(matches!(
        decode_mjpeg_to_rgb24(&[], &mut mjpeg_out),
        Err(VideoIngestionError::EmptyFrame { .. })
    ));
}

#[test]
fn test_adv_bridge_ingest_camera_frame_and_pool_recycling() {
    let pool = Arc::new(FrameBufferPool::new(4, 1024));
    let mut bridge = VideoFrameBridge::with_pool(pool.clone());

    for i in 0..50 {
        let cf = CameraFrame {
            width: 4,
            height: 4,
            format: ImageFormat::Rgb8,
            data: vec![i as u8; 4 * 4 * 3],
            timestamp_ms: i as u64,
        };
        let pooled = bridge.ingest_camera_frame(&cf).expect("CameraFrame ingest ok");
        assert_eq!(pooled.width, 4);
        assert_eq!(pooled.height, 4);
        assert_eq!(pooled.data.len(), 48);
        // At end of iteration, pooled drops and recycles
    }

    let stats = pool.stats();
    assert_eq!(stats.total_acquired, 50);
    assert_eq!(stats.total_recycled, 50);
    assert_eq!(stats.allocations_prevented, 49);
}

#[test]
fn test_adv_bridge_sha256_attestation() {
    let pool = Arc::new(FrameBufferPool::new(2, 512));
    let mut bridge = VideoFrameBridge::with_pool(pool);

    let raw = vec![0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC];
    let mut hasher = Sha256::new();
    hasher.update(&raw);
    let expected: [u8; 32] = hasher.finalize().into();

    let frame = bridge.ingest_raw(&raw, 2, 1, FrameFormat::Rgb8, 0).expect("ingest ok");
    assert_eq!(frame.attestation_hash(), &expected);
}

#[test]
fn test_adv_bridge_multithreaded_concurrent_stress() {
    let pool = Arc::new(FrameBufferPool::new(8, 512));
    let mut handles = Vec::new();

    for t in 0..4 {
        let p = pool.clone();
        handles.push(std::thread::spawn(move || {
            let mut bridge = VideoFrameBridge::with_pool(p);
            for i in 0..100 {
                let raw = vec![(t * 10 + i) as u8; 4 * 4 * 3];
                let f = bridge.ingest_raw(&raw, 4, 4, FrameFormat::Rgb8, i as u64).unwrap();
                assert_eq!(f.data.len(), 48);
            }
        }));
    }

    for h in handles {
        h.join().expect("thread join must not panic");
    }

    let stats = pool.stats();
    assert_eq!(stats.total_acquired, 400);
    assert_eq!(stats.total_recycled, 400);
}
