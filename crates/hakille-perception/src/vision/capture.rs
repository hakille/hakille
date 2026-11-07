//! Abstraction unifiée de capture et d'ingestion vidéo pour Hakille Perception.
//!
//! Ce module implémente le pont universel d'ingestion des trames matérielles
//! ([`hakille_substrate::CameraFrame`]) vers les trames d'analyse normalisées
//! ([`VideoFrame`]), compatible avec tous les substrats (Linux V4L2, Windows
//! MediaFoundation, Mobile Android/iOS, Web Canvas).
//!
//! ### Principes directeurs :
//! 1. **Conversion et décodage déterministes** : YUYV (4:2:2) vers RGB24 par
//!    arithmétique à virgule fixe ultra-rapide (zéro flottant sur le chemin
//!    chaud), MJPEG vers RGB24 par décodage streaming natif, RGBA/BGRA vers
//!    RGB24 sans allocation intermédiaire.
//! 2. **Recyclage de mémoire tampon (Zero-Alloc Steady State)** :
//!    [`FrameBufferPool`] maintient un réservoir de tampons pré-alloués et
//!    [`PooledVideoFrame`] assure la restitution automatique au pool via RAII
//!    lorsque la trame est libérée par les étages d'inférence.
//! 3. **Règle 8 de la Constitution Hakille** :
//!    - Hachage cryptographique SHA-256 d'attestation locale calculé sur chaque
//!      trame brute ingérée.
//!    - Mémoire protégée : écrasement sécurisé volatile (`secure_zeroize`) de
//!      toutes les données de pixels dès leur libération ou recyclage pour
//!      empêcher la persistance résiduelle de données biométriques en RAM.
//!    - Interdiction formelle d'écriture de pixels bruts sur disque.

use hakille_core::Timestamp;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ops::Deref;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Formats d'image pris en charge par le pipeline de capture et d'ingestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FrameFormat {
    /// RGB 24-bit entrelacé (3 octets par pixel : [R, G, B]). Format canonique normalisé.
    Rgb8,
    /// RGBA 32-bit entrelacé (4 octets par pixel : [R, G, B, A]).
    Rgba8,
    /// BGRA 32-bit entrelacé (4 octets par pixel : [B, G, R, A], standard Windows/MediaFoundation).
    Bgra8,
    /// YUYV / YUV 4:2:2 entrelacé (4 octets pour 2 pixels : [Y0, U, Y1, V], standard V4L2 webcam).
    Yuyv,
    /// Motion JPEG / JPEG compressé (standard webcams USB haute résolution).
    Jpeg,
}

impl FrameFormat {
    /// Nombre d'octets par pixel pour les formats non compressés (ou None pour JPEG).
    pub fn bytes_per_pixel(&self) -> Option<usize> {
        match self {
            Self::Rgb8 => Some(3),
            Self::Rgba8 | Self::Bgra8 => Some(4),
            Self::Yuyv => Some(2), // 4 octets pour 2 pixels = 2 octets/pixel
            Self::Jpeg => None,
        }
    }
}

/// Erreurs spécifiques à l'ingestion et la conversion des trames vidéo.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VideoIngestionError {
    #[error("Trame vide ou dimensions nulles : {width}x{height}, données = {data_len} octets")]
    EmptyFrame {
        width: u32,
        height: u32,
        data_len: usize,
    },

    #[error(
        "Sous-dimensionnement du tampon : {expected} octets attendus pour {width}x{height} {format:?}, reçu {actual} octets"
    )]
    BufferUnderflow {
        expected: usize,
        actual: usize,
        width: u32,
        height: u32,
        format: FrameFormat,
    },

    #[error("Échec du décodage JPEG/MJPEG : {0}")]
    JpegDecodeError(String),

    #[error("Format non supporté ou incohérent : {0}")]
    UnsupportedFormat(String),
}

/// Trame vidéo unifiée prête pour l'inférence (généralement normalisée en RGB24).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub format: FrameFormat,
    pub data: Vec<u8>,
    pub timestamp: Timestamp,
}

impl VideoFrame {
    pub fn new(width: u32, height: u32, format: FrameFormat, data: Vec<u8>) -> Self {
        Self {
            width,
            height,
            format,
            data,
            timestamp: Timestamp::now(),
        }
    }

    pub fn aspect_ratio(&self) -> f32 {
        if self.height == 0 {
            1.0
        } else {
            self.width as f32 / self.height as f32
        }
    }
}

/// Écrase de manière sécurisée et volatile une zone mémoire avec des zéros.
///
/// L'utilisation d'écritures volatiles et d'une barrière de compilateur garantit
/// que le compilateur LLVM n'élimine pas cette passe par optimisation de type
/// Dead Store Elimination (DSE). Requis par la Règle 8 de souveraineté.
#[inline]
pub fn secure_zeroize(buf: &mut [u8]) {
    for byte in buf.iter_mut() {
        unsafe {
            std::ptr::write_volatile(byte, 0);
        }
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

/// Métriques et statistiques du pool de mémoire tampon.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PoolStats {
    pub total_acquired: u64,
    pub total_recycled: u64,
    pub allocations_prevented: u64,
    pub current_available: usize,
}

/// Réservoir thread-safe de tampons mémoire réutilisables.
/// Zéro risque d'interblocage (un seul mutex sur `buffers`, statistiques lock-free atomiques).
pub struct FrameBufferPool {
    buffers: Mutex<Vec<Vec<u8>>>,
    max_buffers: usize,
    default_capacity: usize,
    total_acquired: AtomicU64,
    total_recycled: AtomicU64,
    allocations_prevented: AtomicU64,
    current_available: AtomicUsize,
}

impl FrameBufferPool {
    pub fn new(max_buffers: usize, default_capacity: usize) -> Self {
        Self {
            buffers: Mutex::new(Vec::with_capacity(max_buffers)),
            max_buffers,
            default_capacity,
            total_acquired: AtomicU64::new(0),
            total_recycled: AtomicU64::new(0),
            allocations_prevented: AtomicU64::new(0),
            current_available: AtomicUsize::new(0),
        }
    }

    /// Acquiert un tampon d'au moins `min_capacity` octets.
    /// Réutilise un tampon disponible si existant (zéro allocation), sinon alloue.
    pub fn acquire(&self, min_capacity: usize) -> Vec<u8> {
        self.total_acquired.fetch_add(1, Ordering::Relaxed);
        let mut lock = self.buffers.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(pos) = lock.iter().position(|b| b.capacity() >= min_capacity) {
            let mut buf = lock.swap_remove(pos);
            self.current_available.store(lock.len(), Ordering::Relaxed);
            drop(lock);
            self.allocations_prevented.fetch_add(1, Ordering::Relaxed);
            buf.clear();
            buf
        } else {
            self.current_available.store(lock.len(), Ordering::Relaxed);
            drop(lock);
            Vec::with_capacity(min_capacity.max(self.default_capacity))
        }
    }

    /// Recycle un tampon en le remettant dans le réservoir après effacement sécurisé.
    pub fn recycle(&self, mut buffer: Vec<u8>) {
        // Règle 8 : effacer les données de pixels avant toute réutilisation
        secure_zeroize(&mut buffer);
        buffer.clear();

        self.total_recycled.fetch_add(1, Ordering::Relaxed);
        let mut lock = self.buffers.lock().unwrap_or_else(|e| e.into_inner());
        if lock.len() < self.max_buffers {
            lock.push(buffer);
        }
        self.current_available.store(lock.len(), Ordering::Relaxed);
    }

    /// Retourne une copie des métriques actuelles du pool (lecture sans verrou).
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            total_acquired: self.total_acquired.load(Ordering::Relaxed),
            total_recycled: self.total_recycled.load(Ordering::Relaxed),
            allocations_prevented: self.allocations_prevented.load(Ordering::Relaxed),
            current_available: self.current_available.load(Ordering::Relaxed),
        }
    }
}

/// Enveloppe RAII pour [`VideoFrame`] garantissant la restitution automatique
/// du tampon sous-jacent au [`FrameBufferPool`] lors du drop.
pub struct PooledVideoFrame {
    frame: Option<VideoFrame>,
    pool: Arc<FrameBufferPool>,
    attestation_hash: [u8; 32],
}

impl PooledVideoFrame {
    pub fn new(frame: VideoFrame, pool: Arc<FrameBufferPool>, attestation_hash: [u8; 32]) -> Self {
        Self {
            frame: Some(frame),
            pool,
            attestation_hash,
        }
    }

    pub fn frame(&self) -> &VideoFrame {
        self.frame.as_ref().expect("frame active")
    }

    pub fn attestation_hash(&self) -> &[u8; 32] {
        &self.attestation_hash
    }

    /// Extrait le [`VideoFrame`] en renonçant au recyclage automatique via le pool.
    pub fn into_video_frame(mut self) -> VideoFrame {
        self.frame.take().expect("frame active")
    }
}

impl Deref for PooledVideoFrame {
    type Target = VideoFrame;

    fn deref(&self) -> &Self::Target {
        self.frame()
    }
}

impl Drop for PooledVideoFrame {
    fn drop(&mut self) {
        if let Some(mut frame) = self.frame.take() {
            let data = std::mem::take(&mut frame.data);
            self.pool.recycle(data);
        }
    }
}

// -----------------------------------------------------------------------------
// ALGORITHMES PURS RUST DE CONVERSION DE PIXELS
// -----------------------------------------------------------------------------

/// Convertit des pixels YUYV (4:2:2) vers RGB24 par arithmétique à virgule fixe BT.601 ($2^{10}$).
///
/// Débit ultra-rapide sans calcul en virgule flottante. Chaque groupe de 4 octets
/// `[Y0, U, Y1, V]` produit 2 pixels RGB `[R0, G0, B0, R1, G1, B1]`.
pub fn yuyv_to_rgb24(
    src: &[u8],
    dst: &mut [u8],
    width: u32,
    height: u32,
) -> Result<(), VideoIngestionError> {
    let num_pixels =
        (width as usize)
            .checked_mul(height as usize)
            .ok_or(VideoIngestionError::EmptyFrame {
                width,
                height,
                data_len: src.len(),
            })?;
    let expected_src = num_pixels.checked_mul(2).unwrap_or(0);
    let expected_dst = num_pixels.checked_mul(3).unwrap_or(0);

    if src.len() < expected_src || dst.len() < expected_dst {
        return Err(VideoIngestionError::BufferUnderflow {
            expected: expected_src,
            actual: src.len(),
            width,
            height,
            format: FrameFormat::Yuyv,
        });
    }

    let pairs = num_pixels / 2;
    for i in 0..pairs {
        let src_idx = i * 4;
        let dst_idx = i * 6;

        let y0 = src[src_idx] as i32;
        let u0 = src[src_idx + 1] as i32;
        let y1 = src[src_idx + 2] as i32;
        let v0 = src[src_idx + 3] as i32;

        let u = u0 - 128;
        let v = v0 - 128;

        // Facteurs BT.601 mis à l'échelle par 1024 (2^10) :
        // R = Y + 1.402 * V       -> 1.402 * 1024 = 1436
        // G = Y - 0.344*U - 0.714*V -> 0.344 * 1024 = 352, 0.714 * 1024 = 731
        // B = Y + 1.772 * U       -> 1.772 * 1024 = 1815
        let r_diff = (1436 * v) >> 10;
        let g_diff = (352 * u + 731 * v) >> 10;
        let b_diff = (1815 * u) >> 10;

        dst[dst_idx] = (y0 + r_diff).clamp(0, 255) as u8;
        dst[dst_idx + 1] = (y0 - g_diff).clamp(0, 255) as u8;
        dst[dst_idx + 2] = (y0 + b_diff).clamp(0, 255) as u8;

        dst[dst_idx + 3] = (y1 + r_diff).clamp(0, 255) as u8;
        dst[dst_idx + 4] = (y1 - g_diff).clamp(0, 255) as u8;
        dst[dst_idx + 5] = (y1 + b_diff).clamp(0, 255) as u8;
    }

    Ok(())
}

/// Convertit des pixels RGBA 32-bit vers RGB 24-bit en retirant le canal alpha.
pub fn rgba_to_rgb24(
    src: &[u8],
    dst: &mut [u8],
    width: u32,
    height: u32,
) -> Result<(), VideoIngestionError> {
    let num_pixels =
        (width as usize)
            .checked_mul(height as usize)
            .ok_or(VideoIngestionError::EmptyFrame {
                width,
                height,
                data_len: src.len(),
            })?;
    let expected_src = num_pixels.checked_mul(4).unwrap_or(0);
    let expected_dst = num_pixels.checked_mul(3).unwrap_or(0);

    if src.len() < expected_src || dst.len() < expected_dst {
        return Err(VideoIngestionError::BufferUnderflow {
            expected: expected_src,
            actual: src.len(),
            width,
            height,
            format: FrameFormat::Rgba8,
        });
    }

    for (s, d) in src
        .chunks_exact(4)
        .take(num_pixels)
        .zip(dst.chunks_exact_mut(3).take(num_pixels))
    {
        d[0] = s[0];
        d[1] = s[1];
        d[2] = s[2];
    }

    Ok(())
}

/// Convertit des pixels BGRA 32-bit vers RGB 24-bit (échange B et R, retrait de l'alpha).
pub fn bgra_to_rgb24(
    src: &[u8],
    dst: &mut [u8],
    width: u32,
    height: u32,
) -> Result<(), VideoIngestionError> {
    let num_pixels =
        (width as usize)
            .checked_mul(height as usize)
            .ok_or(VideoIngestionError::EmptyFrame {
                width,
                height,
                data_len: src.len(),
            })?;
    let expected_src = num_pixels.checked_mul(4).unwrap_or(0);
    let expected_dst = num_pixels.checked_mul(3).unwrap_or(0);

    if src.len() < expected_src || dst.len() < expected_dst {
        return Err(VideoIngestionError::BufferUnderflow {
            expected: expected_src,
            actual: src.len(),
            width,
            height,
            format: FrameFormat::Bgra8,
        });
    }

    for (s, d) in src
        .chunks_exact(4)
        .take(num_pixels)
        .zip(dst.chunks_exact_mut(3).take(num_pixels))
    {
        d[0] = s[2]; // R
        d[1] = s[1]; // G
        d[2] = s[0]; // B
    }

    Ok(())
}

/// Décode une trame MJPEG / JPEG vers RGB24 dans un tampon pré-alloué.
pub fn decode_mjpeg_to_rgb24(
    mjpeg_data: &[u8],
    dst: &mut Vec<u8>,
) -> Result<(u32, u32), VideoIngestionError> {
    if mjpeg_data.is_empty() {
        return Err(VideoIngestionError::EmptyFrame {
            width: 0,
            height: 0,
            data_len: 0,
        });
    }

    let img = image::load_from_memory_with_format(mjpeg_data, image::ImageFormat::Jpeg)
        .map_err(|e| VideoIngestionError::JpegDecodeError(e.to_string()))?;

    let (w, h) = (img.width(), img.height());
    let rgb = img.to_rgb8();
    let raw = rgb.as_raw();

    dst.clear();
    dst.extend_from_slice(raw);

    Ok((w, h))
}

// -----------------------------------------------------------------------------
// PONT UNIVERSEL D'INGESTION VIDÉO
// -----------------------------------------------------------------------------

/// Pont universel d'ingestion de trames vidéo pour Hakille Perception.
///
/// Assure la liaison entre les abstractions matérielles d'OS ([`hakille_substrate::CameraFrame`])
/// et les modules d'inférence/analyse, en garantissant le zéro-allocation en régime
/// permanent et la conformité absolue à la Règle 8 de souveraineté.
pub struct VideoFrameBridge {
    pool: Arc<FrameBufferPool>,
    frames_ingested: u64,
}

impl VideoFrameBridge {
    /// Crée une nouvelle instance avec un pool par défaut (jusqu'à 8 trames 1080p RGB).
    pub fn new(max_pooled_buffers: usize) -> Self {
        // 1920 * 1080 * 3 ~ 6.22 Mo par tampon par défaut
        Self::with_pool(Arc::new(FrameBufferPool::new(
            max_pooled_buffers,
            1920 * 1080 * 3,
        )))
    }

    /// Crée une instance avec un pool de tampons personnalisé.
    pub fn with_pool(pool: Arc<FrameBufferPool>) -> Self {
        Self {
            pool,
            frames_ingested: 0,
        }
    }

    pub fn pool(&self) -> &Arc<FrameBufferPool> {
        &self.pool
    }

    pub fn frames_ingested(&self) -> u64 {
        self.frames_ingested
    }

    /// Ingère un tampon brut de n'importe quel format et le convertit en [`PooledVideoFrame`] RGB24.
    pub fn ingest_raw(
        &mut self,
        raw_data: &[u8],
        width: u32,
        height: u32,
        format: FrameFormat,
        _timestamp_ms: u64,
    ) -> Result<PooledVideoFrame, VideoIngestionError> {
        self.frames_ingested += 1;

        if width == 0 || height == 0 || raw_data.is_empty() {
            return Err(VideoIngestionError::EmptyFrame {
                width,
                height,
                data_len: raw_data.len(),
            });
        }

        // Règle 8 : Calcul déterministe de l'attestation cryptographique SHA-256
        let mut hasher = Sha256::new();
        hasher.update(raw_data);
        let attestation_hash: [u8; 32] = hasher.finalize().into();

        // Conversion vers RGB24 normalisé en réutilisant le pool
        let (out_w, out_h, rgb_buffer) = match format {
            FrameFormat::Rgb8 => {
                let required = (width as usize)
                    .checked_mul(height as usize)
                    .and_then(|n| n.checked_mul(3))
                    .ok_or(VideoIngestionError::EmptyFrame {
                        width,
                        height,
                        data_len: raw_data.len(),
                    })?;
                if raw_data.len() < required {
                    return Err(VideoIngestionError::BufferUnderflow {
                        expected: required,
                        actual: raw_data.len(),
                        width,
                        height,
                        format,
                    });
                }
                let mut buf = self.pool.acquire(required);
                buf.clear();
                buf.extend_from_slice(&raw_data[..required]);
                (width, height, buf)
            }
            FrameFormat::Rgba8 => {
                let num_px = (width as usize).checked_mul(height as usize).ok_or(
                    VideoIngestionError::EmptyFrame {
                        width,
                        height,
                        data_len: raw_data.len(),
                    },
                )?;
                let required_src = num_px * 4;
                let required_dst = num_px * 3;
                if raw_data.len() < required_src {
                    return Err(VideoIngestionError::BufferUnderflow {
                        expected: required_src,
                        actual: raw_data.len(),
                        width,
                        height,
                        format,
                    });
                }
                let mut buf = self.pool.acquire(required_dst);
                buf.resize(required_dst, 0);
                rgba_to_rgb24(raw_data, &mut buf, width, height)?;
                (width, height, buf)
            }
            FrameFormat::Bgra8 => {
                let num_px = (width as usize).checked_mul(height as usize).ok_or(
                    VideoIngestionError::EmptyFrame {
                        width,
                        height,
                        data_len: raw_data.len(),
                    },
                )?;
                let required_src = num_px * 4;
                let required_dst = num_px * 3;
                if raw_data.len() < required_src {
                    return Err(VideoIngestionError::BufferUnderflow {
                        expected: required_src,
                        actual: raw_data.len(),
                        width,
                        height,
                        format,
                    });
                }
                let mut buf = self.pool.acquire(required_dst);
                buf.resize(required_dst, 0);
                bgra_to_rgb24(raw_data, &mut buf, width, height)?;
                (width, height, buf)
            }
            FrameFormat::Yuyv => {
                let num_px = (width as usize).checked_mul(height as usize).ok_or(
                    VideoIngestionError::EmptyFrame {
                        width,
                        height,
                        data_len: raw_data.len(),
                    },
                )?;
                let required_src = num_px * 2;
                let required_dst = num_px * 3;
                if raw_data.len() < required_src {
                    return Err(VideoIngestionError::BufferUnderflow {
                        expected: required_src,
                        actual: raw_data.len(),
                        width,
                        height,
                        format,
                    });
                }
                let mut buf = self.pool.acquire(required_dst);
                buf.resize(required_dst, 0);
                yuyv_to_rgb24(raw_data, &mut buf, width, height)?;
                (width, height, buf)
            }
            FrameFormat::Jpeg => {
                let mut buf = self.pool.acquire((width as usize) * (height as usize) * 3);
                let (w, h) = decode_mjpeg_to_rgb24(raw_data, &mut buf)?;
                (w, h, buf)
            }
        };

        let video_frame = VideoFrame {
            width: out_w,
            height: out_h,
            format: FrameFormat::Rgb8,
            data: rgb_buffer,
            timestamp: Timestamp::now(),
        };

        Ok(PooledVideoFrame::new(
            video_frame,
            self.pool.clone(),
            attestation_hash,
        ))
    }

    /// Ingère directement une [`hakille_substrate::CameraFrame`].
    pub fn ingest_camera_frame(
        &mut self,
        frame: &hakille_substrate::CameraFrame,
    ) -> Result<PooledVideoFrame, VideoIngestionError> {
        let format = match frame.format {
            hakille_substrate::ImageFormat::Rgb8 => FrameFormat::Rgb8,
            hakille_substrate::ImageFormat::Rgba8 => FrameFormat::Rgba8,
            hakille_substrate::ImageFormat::Bgra8 => FrameFormat::Bgra8,
        };
        self.ingest_raw(
            &frame.data,
            frame.width,
            frame.height,
            format,
            frame.timestamp_ms,
        )
    }
}

// -----------------------------------------------------------------------------
// TESTS UNITAIRES
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yuyv_to_rgb24_known_values() {
        // 2 pixels : Pixel 0 et Pixel 1
        // [Y0, U, Y1, V]
        // 1. Blanc pur : Y=255, U=128, Y=255, V=128
        let white_yuyv = [255u8, 128, 255, 128];
        let mut white_rgb = [0u8; 6];
        yuyv_to_rgb24(&white_yuyv, &mut white_rgb, 2, 1).expect("conversion ok");
        assert_eq!(white_rgb, [255, 255, 255, 255, 255, 255]);

        // 2. Noir pur : Y=0, U=128, Y=0, V=128
        let black_yuyv = [0u8, 128, 0, 128];
        let mut black_rgb = [0u8; 6];
        yuyv_to_rgb24(&black_yuyv, &mut black_rgb, 2, 1).expect("conversion ok");
        assert_eq!(black_rgb, [0, 0, 0, 0, 0, 0]);

        // 3. Gris neutre : Y=128, U=128, Y=128, V=128
        let gray_yuyv = [128u8, 128, 128, 128];
        let mut gray_rgb = [0u8; 6];
        yuyv_to_rgb24(&gray_yuyv, &mut gray_rgb, 2, 1).expect("conversion ok");
        assert_eq!(gray_rgb, [128, 128, 128, 128, 128, 128]);

        // 4. Rouge approché : Y=76, U=85, Y=76, V=255
        let red_yuyv = [76u8, 85, 76, 255];
        let mut red_rgb = [0u8; 6];
        yuyv_to_rgb24(&red_yuyv, &mut red_rgb, 2, 1).expect("conversion ok");
        assert!(red_rgb[0] >= 250, "R fort: {}", red_rgb[0]);
        assert!(red_rgb[1] <= 5, "G faible: {}", red_rgb[1]);
        assert!(red_rgb[2] <= 5, "B faible: {}", red_rgb[2]);
    }

    #[test]
    fn test_rgba_and_bgra_to_rgb24() {
        let rgba = [10u8, 20, 30, 255, 40, 50, 60, 255];
        let mut rgb1 = [0u8; 6];
        rgba_to_rgb24(&rgba, &mut rgb1, 2, 1).expect("rgba ok");
        assert_eq!(rgb1, [10, 20, 30, 40, 50, 60]);

        // Pour BGRA : B=30, G=20, R=10 -> doit produire R=10, G=20, B=30
        let bgra = [30u8, 20, 10, 255, 60, 50, 40, 255];
        let mut rgb2 = [0u8; 6];
        bgra_to_rgb24(&bgra, &mut rgb2, 2, 1).expect("bgra ok");
        assert_eq!(rgb2, [10, 20, 30, 40, 50, 60]);
    }

    #[test]
    fn test_buffer_pool_zero_alloc_steady_state() {
        let pool = Arc::new(FrameBufferPool::new(4, 1024));
        assert_eq!(pool.stats().current_available, 0);

        // Premier cycle : acquisition
        let buf1 = pool.acquire(512);
        assert_eq!(pool.stats().total_acquired, 1);
        assert_eq!(pool.stats().allocations_prevented, 0);

        // Recyclage
        pool.recycle(buf1);
        assert_eq!(pool.stats().total_recycled, 1);
        assert_eq!(pool.stats().current_available, 1);

        // Deuxième cycle : réutilisation (zéro allocation !)
        let buf2 = pool.acquire(512);
        assert_eq!(pool.stats().total_acquired, 2);
        assert_eq!(pool.stats().allocations_prevented, 1);
        assert_eq!(pool.stats().current_available, 0);

        pool.recycle(buf2);
        assert_eq!(pool.stats().current_available, 1);
    }

    #[test]
    fn test_pooled_video_frame_drop_auto_recycle() {
        let pool = Arc::new(FrameBufferPool::new(4, 1024));
        let mut bridge = VideoFrameBridge::with_pool(pool.clone());

        {
            let raw = vec![128u8; 640 * 480 * 2]; // YUYV
            let pooled_frame = bridge
                .ingest_raw(&raw, 640, 480, FrameFormat::Yuyv, 100)
                .expect("ingest ok");
            assert_eq!(pooled_frame.width, 640);
            assert_eq!(pooled_frame.height, 480);
            assert_eq!(pooled_frame.format, FrameFormat::Rgb8);
            assert_eq!(pooled_frame.data.len(), 640 * 480 * 3);
            assert_eq!(pool.stats().total_acquired, 1);
            assert_eq!(pool.stats().total_recycled, 0);
        } // pooled_frame est libéré ici -> Drop doit recycler dans le pool

        assert_eq!(
            pool.stats().total_recycled,
            1,
            "Le buffer doit avoir été recyclé automatiquement au drop"
        );
        assert_eq!(pool.stats().current_available, 1);

        // Trame suivante : doit réutiliser le buffer précédent
        {
            let raw = vec![128u8; 640 * 480 * 2];
            let _frame2 = bridge
                .ingest_raw(&raw, 640, 480, FrameFormat::Yuyv, 200)
                .expect("ingest ok");
            assert_eq!(pool.stats().total_acquired, 2);
            assert_eq!(pool.stats().allocations_prevented, 1);
        }
        assert_eq!(pool.stats().total_recycled, 2);
    }

    #[test]
    fn test_rule_8_secure_zeroize() {
        let mut sensitive_pixels = vec![0xAAu8; 4096];
        assert_eq!(sensitive_pixels[0], 0xAA);

        secure_zeroize(&mut sensitive_pixels);

        for &byte in &sensitive_pixels {
            assert_eq!(
                byte, 0,
                "Chaque octet de pixel doit être à 0 après secure_zeroize"
            );
        }
    }

    #[test]
    fn test_rule_8_sha256_attestation() {
        let pool = Arc::new(FrameBufferPool::new(2, 512));
        let mut bridge = VideoFrameBridge::with_pool(pool);

        let raw_data = vec![42u8; 100];
        let mut hasher = Sha256::new();
        hasher.update(&raw_data);
        let expected_hash: [u8; 32] = hasher.finalize().into();

        // Créer un frame RGBA valide 5x5
        let rgba_data = vec![120u8; 5 * 5 * 4];
        let mut expected_rgba_hasher = Sha256::new();
        expected_rgba_hasher.update(&rgba_data);
        let expected_rgba_hash: [u8; 32] = expected_rgba_hasher.finalize().into();

        let frame = bridge
            .ingest_raw(&rgba_data, 5, 5, FrameFormat::Rgba8, 500)
            .expect("ingest ok");
        assert_eq!(frame.attestation_hash(), &expected_rgba_hash);

        // Tester également une trame RGB standard
        let rgb_data = vec![77u8; 2 * 2 * 3];
        let mut expected_rgb_hasher = Sha256::new();
        expected_rgb_hasher.update(&rgb_data);
        let expected_rgb_hash: [u8; 32] = expected_rgb_hasher.finalize().into();

        let frame_rgb = bridge
            .ingest_raw(&rgb_data, 2, 2, FrameFormat::Rgb8, 600)
            .expect("ingest ok");
        assert_eq!(frame_rgb.attestation_hash(), &expected_rgb_hash);
        let _ = expected_hash;
    }

    #[test]
    fn test_error_resilience() {
        let pool = Arc::new(FrameBufferPool::new(2, 512));
        let mut bridge = VideoFrameBridge::with_pool(pool);

        // 1. Image vide
        let err_empty = bridge.ingest_raw(&[], 0, 0, FrameFormat::Rgb8, 0);
        assert!(matches!(
            err_empty,
            Err(VideoIngestionError::EmptyFrame { .. })
        ));

        // 2. Tampon tronqué (buffer underflow)
        let truncated = vec![10u8; 10];
        let err_underflow = bridge.ingest_raw(&truncated, 640, 480, FrameFormat::Yuyv, 0);
        assert!(matches!(
            err_underflow,
            Err(VideoIngestionError::BufferUnderflow { .. })
        ));

        // 3. Données JPEG corrompues
        let corrupted_jpeg = vec![0xFF, 0xD8, 0x00, 0x01, 0x02];
        let err_jpeg = bridge.ingest_raw(&corrupted_jpeg, 100, 100, FrameFormat::Jpeg, 0);
        assert!(matches!(
            err_jpeg,
            Err(VideoIngestionError::JpegDecodeError(..))
        ));
    }
}
