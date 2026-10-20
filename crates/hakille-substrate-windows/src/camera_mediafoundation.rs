//! Capture caméra Windows réelle via MediaFoundation (IMFSourceReader).
//!
//! Aucune frame synthétique : toute indisponibilité (pas de device, accès
//! refusé, backend absent) remonte une erreur honnête nommant MediaFoundation.
//! Le COM (MTA) et l'IMFSourceReader appartiennent à un thread dédié ; le
//! reste du processus ne voit qu'un canal watch « dernière frame » (latest).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use hakille_substrate::{CameraConfig, CameraFacing, CameraFrame, ImageFormat};
use windows::Win32::Media::MediaFoundation::{
    IMFActivate, IMFAttributes, IMFMediaSource, IMFMediaType, IMFSample, IMFSourceReader,
    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
    MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE,
    MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING,
    MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_SOURCE_READERF_ENDOFSTREAM,
    MF_SOURCE_READERF_STREAMTICK, MF_VERSION, MFCreateAttributes, MFCreateMediaType,
    MFCreateSourceReaderFromMediaSource, MFEnumDeviceSources, MFMediaType_Video, MFSTARTUP_FULL,
    MFShutdown, MFStartup, MFVideoFormat_RGB32, MFVideoInterlace_Progressive,
};
use windows::Win32::System::Com::{
    COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};

const INIT_TIMEOUT: Duration = Duration::from_secs(15);
const FIRST_FRAME_DEADLINE: Duration = Duration::from_secs(10);
const STOP_TIMEOUT: Duration = Duration::from_secs(2);

struct ComGuard;

impl ComGuard {
    fn new() -> Result<Self, String> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(|e| format!("camera unavailable: MediaFoundation COM init failed: {e}"))?;
        }
        Ok(Self)
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

struct MfGuard;

impl MfGuard {
    fn new() -> Result<Self, String> {
        unsafe {
            MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(|e| {
                format!("camera unavailable: MediaFoundation MFStartup failed: {e}")
            })?;
        }
        Ok(Self)
    }
}

impl Drop for MfGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
        }
    }
}

struct ActiveCapture {
    reader: IMFSourceReader,
    source: IMFMediaSource,
    stream_index: u32,
    width: u32,
    height: u32,
    stride: i32,
    // Déclarés après reader/source : MF reste initialisé pendant que les
    // objets COM sont relâchés (ordre de drop = ordre de déclaration).
    _mf: MfGuard,
    _com: ComGuard,
}

fn open_capture(config: &CameraConfig) -> Result<ActiveCapture, String> {
    let com = ComGuard::new()?;
    let mf = MfGuard::new()?;

    let attributes = unsafe {
        let mut attributes: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attributes, 1).map_err(|e| {
            format!("camera unavailable: MediaFoundation MFCreateAttributes failed: {e}")
        })?;
        attributes.ok_or_else(|| {
            "camera unavailable: MediaFoundation MFCreateAttributes returned null".to_string()
        })?
    };

    unsafe {
        attributes
            .SetGUID(
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
            )
            .map_err(|e| {
                format!("camera unavailable: MediaFoundation device enumeration failed: {e}")
            })?;
    }

    let activate = unsafe {
        let mut devices: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count: u32 = 0;
        MFEnumDeviceSources(&attributes, &mut devices, &mut count).map_err(|e| {
            format!("camera unavailable: MediaFoundation MFEnumDeviceSources failed: {e}")
        })?;
        if count == 0 {
            return Err(
                "camera unavailable: MediaFoundation found no video capture device".to_string(),
            );
        }
        let slots = std::slice::from_raw_parts_mut(devices, count as usize);
        let index = match config.facing {
            CameraFacing::External if slots.len() > 1 => 1,
            _ => 0,
        };
        let chosen = slots[index].clone();
        for slot in slots.iter_mut() {
            *slot = None;
        }
        CoTaskMemFree(Some(devices as *const core::ffi::c_void));
        chosen.ok_or_else(|| {
            "camera unavailable: MediaFoundation device activation handle empty".to_string()
        })?
    };

    let source: IMFMediaSource = unsafe {
        activate.ActivateObject().map_err(|e| {
            format!("camera unavailable: MediaFoundation source activation failed: {e}")
        })?
    };

    let reader_attributes = unsafe {
        let mut attributes: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attributes, 1).map_err(|e| {
            format!("camera unavailable: MediaFoundation reader attributes failed: {e}")
        })?;
        let attributes = attributes.ok_or_else(|| {
            "camera unavailable: MediaFoundation MFCreateAttributes returned null".to_string()
        })?;
        attributes
            .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
            .map_err(|e| {
                format!("camera unavailable: MediaFoundation video processing toggle failed: {e}")
            })?;
        attributes
    };

    let reader = unsafe {
        MFCreateSourceReaderFromMediaSource(&source, &reader_attributes).map_err(|e| {
            format!("camera unavailable: MediaFoundation source reader creation failed: {e}")
        })?
    };

    let stream_index = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

    if set_rgb32_media_type(
        &reader,
        stream_index,
        Some((config.width, config.height, config.fps)),
    )
    .is_err()
    {
        set_rgb32_media_type(&reader, stream_index, None).map_err(|e| {
            format!("camera unavailable: MediaFoundation could not set RGB32 output: {e}")
        })?;
    }

    let (width, height, stride) = unsafe {
        let current = reader.GetCurrentMediaType(stream_index).map_err(|e| {
            format!("camera unavailable: MediaFoundation current media type query failed: {e}")
        })?;
        let packed = current.GetUINT64(&MF_MT_FRAME_SIZE).map_err(|e| {
            format!("camera unavailable: MediaFoundation frame size query failed: {e}")
        })?;
        let width = (packed >> 32) as u32;
        let height = (packed & 0xFFFF_FFFF) as u32;
        let stride = current
            .GetUINT32(&MF_MT_DEFAULT_STRIDE)
            .ok()
            .map(|value| value as i32)
            .filter(|value| *value != 0)
            .unwrap_or(width as i32 * 4);
        (width, height, stride)
    };

    if width == 0 || height == 0 {
        return Err(
            "camera unavailable: MediaFoundation negotiated an empty frame size".to_string(),
        );
    }

    Ok(ActiveCapture {
        reader,
        source,
        stream_index,
        width,
        height,
        stride,
        _mf: mf,
        _com: com,
    })
}

fn set_rgb32_media_type(
    reader: &IMFSourceReader,
    stream_index: u32,
    target: Option<(u32, u32, u32)>,
) -> windows::core::Result<()> {
    unsafe {
        let media_type: IMFMediaType = MFCreateMediaType()?;
        media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        media_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
        if let Some((width, height, fps)) = target {
            media_type.SetUINT64(&MF_MT_FRAME_SIZE, ((width as u64) << 32) | height as u64)?;
            media_type.SetUINT64(&MF_MT_FRAME_RATE, ((fps as u64) << 32) | 1)?;
            media_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        }
        reader.SetCurrentMediaType(stream_index, None, &media_type)
    }
}

fn sample_to_frame(
    sample: &IMFSample,
    width: u32,
    height: u32,
    stride: i32,
    timestamp_100ns: i64,
) -> Option<CameraFrame> {
    let row_bytes = width as usize * 4;
    let mut data = vec![0u8; row_bytes * height as usize];
    unsafe {
        let buffer = sample.ConvertToContiguousBuffer().ok()?;
        let mut pointer: *mut u8 = std::ptr::null_mut();
        let mut max_length: u32 = 0;
        let mut current_length: u32 = 0;
        buffer
            .Lock(
                &mut pointer,
                Some(std::ptr::addr_of_mut!(max_length)),
                Some(std::ptr::addr_of_mut!(current_length)),
            )
            .ok()?;
        let available = current_length as usize;
        let source_stride = if stride == 0 {
            row_bytes as i32
        } else {
            stride
        };
        let step = source_stride.unsigned_abs() as usize;
        for row in 0..height as usize {
            let source_row = if source_stride < 0 {
                height as usize - 1 - row
            } else {
                row
            };
            let source_offset = source_row * step;
            if source_offset >= available {
                break;
            }
            let copy_len = row_bytes.min(available - source_offset);
            std::ptr::copy_nonoverlapping(
                pointer.add(source_offset),
                data.as_mut_ptr().add(row * row_bytes),
                copy_len,
            );
        }
        let _ = buffer.Unlock();
    }
    Some(CameraFrame {
        width,
        height,
        data,
        format: ImageFormat::Bgra8,
        timestamp_ms: (timestamp_100ns.max(0) as u64) / 10_000,
    })
}

fn capture_loop(
    config: CameraConfig,
    stop: Arc<AtomicBool>,
    watch_tx: tokio::sync::watch::Sender<Option<CameraFrame>>,
    init_tx: mpsc::SyncSender<Result<(), String>>,
    exit_tx: mpsc::Sender<()>,
) {
    let capture = match open_capture(&config) {
        Ok(capture) => capture,
        Err(message) => {
            let _ = init_tx.send(Err(message));
            let _ = exit_tx.send(());
            return;
        }
    };

    let mut initialized = false;
    let started = Instant::now();

    while !stop.load(Ordering::SeqCst) {
        if !initialized && started.elapsed() > FIRST_FRAME_DEADLINE {
            break;
        }

        let mut actual_stream: u32 = 0;
        let mut flags: u32 = 0;
        let mut timestamp: i64 = 0;
        let mut sample: Option<IMFSample> = None;
        let read_result = unsafe {
            capture.reader.ReadSample(
                capture.stream_index,
                0,
                Some(std::ptr::addr_of_mut!(actual_stream)),
                Some(std::ptr::addr_of_mut!(flags)),
                Some(std::ptr::addr_of_mut!(timestamp)),
                Some(std::ptr::addr_of_mut!(sample)),
            )
        };
        if read_result.is_err() {
            break;
        }
        if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
            break;
        }
        if flags & MF_SOURCE_READERF_STREAMTICK.0 as u32 != 0 {
            continue;
        }
        let Some(sample) = sample else {
            continue;
        };
        let Some(frame) = sample_to_frame(
            &sample,
            capture.width,
            capture.height,
            capture.stride,
            timestamp,
        ) else {
            continue;
        };
        if watch_tx.send(Some(frame)).is_err() {
            break;
        }
        if !initialized {
            initialized = true;
            if init_tx.send(Ok(())).is_err() {
                break;
            }
        }
    }

    if !initialized {
        let _ = init_tx.send(Err(
            "camera unavailable: MediaFoundation produced no frame (privacy permission denied or device busy)"
                .to_string(),
        ));
    }

    unsafe {
        let _ = capture.reader.Flush(capture.stream_index);
        let _ = capture.source.Shutdown();
    }
    let _ = exit_tx.send(());
}

/// Flux caméra actif. Le thread propriétaire du reader MF meurt avec le flux.
pub struct CameraStream {
    stop: Arc<AtomicBool>,
    exited: mpsc::Receiver<()>,
    latest: tokio::sync::watch::Receiver<Option<CameraFrame>>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl CameraStream {
    /// Récepteur watch « dernière frame » (latest-only, pas de backlog).
    pub fn latest(&self) -> tokio::sync::watch::Receiver<Option<CameraFrame>> {
        self.latest.clone()
    }

    /// Arrête le thread (au prochain sample) puis rejoint avec un timeout ;
    /// au-delà, le thread est détaché et mourra à la prochaine frame.
    pub fn stop_and_join(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let exited = self.exited.recv_timeout(STOP_TIMEOUT).is_ok();
        if let Some(join) = self.join.take().filter(|_| exited) {
            let _ = join.join();
        }
    }
}

/// Démarre la capture réelle. Ne retourne `Ok` qu'après la première frame
/// effectivement lue du device (handshake) — sinon erreur honnête.
pub fn start(config: &CameraConfig) -> Result<CameraStream, String> {
    let (init_tx, init_rx) = mpsc::sync_channel::<Result<(), String>>(1);
    let (watch_tx, watch_rx) = tokio::sync::watch::channel(None::<CameraFrame>);
    let (exit_tx, exit_rx) = mpsc::channel::<()>();
    let stop = Arc::new(AtomicBool::new(false));

    let thread_stop = Arc::clone(&stop);
    let thread_config = config.clone();
    let join = std::thread::Builder::new()
        .name("hakille-mf-camera".to_string())
        .spawn(move || capture_loop(thread_config, thread_stop, watch_tx, init_tx, exit_tx))
        .map_err(|e| {
            format!("camera unavailable: MediaFoundation capture thread spawn failed: {e}")
        })?;

    match init_rx.recv_timeout(INIT_TIMEOUT) {
        Ok(Ok(())) => Ok(CameraStream {
            stop,
            exited: exit_rx,
            latest: watch_rx,
            join: Some(join),
        }),
        Ok(Err(message)) => {
            let _ = join.join();
            Err(message)
        }
        Err(_) => {
            stop.store(true, Ordering::SeqCst);
            drop(join);
            Err("camera unavailable: MediaFoundation init timed out".to_string())
        }
    }
}

/// Attend une frame (fast path: la dernière connue), timeout inclus.
pub async fn next_frame(
    mut receiver: tokio::sync::watch::Receiver<Option<CameraFrame>>,
    timeout: Duration,
) -> Result<CameraFrame, String> {
    loop {
        // Clone immédiat : le Ref (non-Send) tombe à la fin de l'instruction,
        // avant l'await sur `changed()`.
        let current: Option<CameraFrame> = (*receiver.borrow_and_update()).clone();
        if let Some(frame) = current {
            return Ok(frame);
        }
        match tokio::time::timeout(timeout, receiver.changed()).await {
            Ok(Ok(())) => continue,
            Ok(Err(_)) => {
                return Err(
                    "camera capture failed: MediaFoundation stream ended (device lost or stopped)"
                        .to_string(),
                );
            }
            Err(_) => {
                return Err(
                    "camera capture failed: MediaFoundation no frame within timeout".to_string(),
                );
            }
        }
    }
}
