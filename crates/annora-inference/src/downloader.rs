use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use annora_core::{AnnoraError, Result};
use futures_util::StreamExt;
use reqwest::header::{CONTENT_LENGTH, RANGE};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

/// Progress statistics emitted periodically during model artifact downloads.
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub filename: String,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
    pub speed_bytes_per_sec: f64,
    pub percentage: Option<f64>,
}

/// Type alias for thread-safe download progress observers.
pub type ProgressCallback = Arc<dyn Fn(&DownloadProgress) + Send + Sync>;

/// Sovereign Model Downloader with streaming chunk retrieval, SHA-256 cryptographic verification,
/// automatic resume support via HTTP Range requests, and atomic file promotion (.part -> final).
#[derive(Clone)]
pub struct ModelDownloader {
    client: reqwest::Client,
}

impl Default for ModelDownloader {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelDownloader {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("Annora-Cognitive-OS/0.1.0 (Zero-Touch Native Autonomous Runtime)")
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self { client }
    }

    /// Download an artifact directly from a URL to the specified destination path.
    ///
    /// - If the destination file already exists and valid, skips download.
    /// - Writes to `{destination}.part` during transit.
    /// - Resumes partial downloads if supported by upstream server.
    /// - Computes incremental SHA-256 and verifies against expected checksum.
    /// - Atomically renames `{destination}.part` to `destination` upon success.
    pub async fn download_file(
        &self,
        url: &str,
        destination: &Path,
        expected_size: Option<u64>,
        expected_sha256: Option<&str>,
        progress: Option<ProgressCallback>,
    ) -> Result<PathBuf> {
        let filename = destination
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "model_artifact".to_string());

        // 1. Check if destination already exists and is valid
        if destination.exists() {
            if let Ok(metadata) = std::fs::metadata(destination) {
                let actual_size = metadata.len();
                let size_ok = expected_size.map(|s| s == actual_size).unwrap_or(true);

                if size_ok {
                    if let Some(expected_hash) = expected_sha256 {
                        if Self::verify_sha256(destination, expected_hash)? {
                            tracing::info!(
                                file = %filename,
                                "Model artifact already present on disk with matching SHA-256 checksum, skipping download"
                            );
                            return Ok(destination.to_path_buf());
                        }
                    } else if actual_size > 0 {
                        tracing::info!(
                            file = %filename,
                            size_bytes = actual_size,
                            "Model artifact already present on disk, skipping download"
                        );
                        return Ok(destination.to_path_buf());
                    }
                }
            }
        }

        // Ensure parent directory exists
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AnnoraError::StorageError(format!("Failed to create directory {}: {}", parent.display(), e)))?;
        }

        let part_path = destination.with_extension("part");

        // 2. Check if a partial file exists for resume
        let mut existing_bytes: u64 = 0;
        let mut hasher = Sha256::new();

        if part_path.exists() {
            if let Ok(metadata) = std::fs::metadata(&part_path) {
                let size = metadata.len();
                // If partial file exists and not empty, hash the existing portion to allow SHA256 continuity
                if size > 0 && expected_size.map(|exp| size < exp).unwrap_or(true) {
                    tracing::info!(file = %filename, bytes = size, "Resuming partial model artifact download");
                    match Self::hash_file_partial(&part_path, size) {
                        Ok((bytes_read, pre_hasher)) if bytes_read == size => {
                            existing_bytes = size;
                            hasher = pre_hasher;
                        }
                        _ => {
                            // If partial read failed, start fresh
                            let _ = tokio::fs::remove_file(&part_path).await;
                            existing_bytes = 0;
                            hasher = Sha256::new();
                        }
                    }
                }
            }
        }

        // 3. Initiate HTTP Request
        let mut request = self.client.get(url);
        if existing_bytes > 0 {
            request = request.header(RANGE, format!("bytes={}-", existing_bytes));
        }

        let response = request
            .send()
            .await
            .map_err(|e| AnnoraError::InferenceError(format!("Network download request failed for {}: {}", url, e)))?;

        let status = response.status();
        let (mut file, mut downloaded_so_far, total_bytes) = if existing_bytes > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT {
            // Resume accepted
            let content_len = response.headers().get(CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok());
            let total = content_len.map(|cl| cl + existing_bytes).or(expected_size);

            let f = tokio::fs::OpenOptions::new()
                .write(true)
                .append(true)
                .open(&part_path)
                .await
                .map_err(|e| AnnoraError::StorageError(format!("Failed to open .part file for append: {}", e)))?;

            (f, existing_bytes, total)
        } else {
            // Fresh download or server does not support Range
            if !status.is_success() {
                return Err(AnnoraError::InferenceError(format!(
                    "HTTP request failed with status code {} for URL: {}",
                    status, url
                )));
            }

            hasher = Sha256::new();
            let total = response.headers().get(CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .or(expected_size);

            let f = tokio::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&part_path)
                .await
                .map_err(|e| AnnoraError::StorageError(format!("Failed to create .part file: {}", e)))?;

            (f, 0u64, total)
        };

        // 4. Stream response body
        let mut stream = response.bytes_stream();
        let start_time = Instant::now();
        let mut last_progress_time = Instant::now();

        while let Some(chunk_res) = stream.next().await {
            let chunk = chunk_res.map_err(|e| {
                AnnoraError::InferenceError(format!("Error streaming chunk for {}: {}", filename, e))
            })?;

            file.write_all(&chunk)
                .await
                .map_err(|e| AnnoraError::StorageError(format!("Failed to write chunk to disk: {}", e)))?;

            hasher.update(&chunk);
            downloaded_so_far += chunk.len() as u64;

            // Notify progress throttled to 100ms
            if let Some(ref cb) = progress {
                if last_progress_time.elapsed().as_millis() >= 100 {
                    let elapsed = start_time.elapsed().as_secs_f64().max(0.001);
                    let speed = (downloaded_so_far - existing_bytes) as f64 / elapsed;
                    let percentage = total_bytes.map(|t| (downloaded_so_far as f64 / t as f64) * 100.0);

                    cb(&DownloadProgress {
                        filename: filename.clone(),
                        bytes_downloaded: downloaded_so_far,
                        total_bytes,
                        speed_bytes_per_sec: speed,
                        percentage,
                    });
                    last_progress_time = Instant::now();
                }
            }
        }

        // Flush and close file
        file.flush()
            .await
            .map_err(|e| AnnoraError::StorageError(format!("Failed to flush .part file: {}", e)))?;
        drop(file);

        // 5. Final progress report (100%)
        if let Some(ref cb) = progress {
            let elapsed = start_time.elapsed().as_secs_f64().max(0.001);
            let speed = (downloaded_so_far - existing_bytes) as f64 / elapsed;
            cb(&DownloadProgress {
                filename: filename.clone(),
                bytes_downloaded: downloaded_so_far,
                total_bytes: Some(downloaded_so_far),
                speed_bytes_per_sec: speed,
                percentage: Some(100.0),
            });
        }

        // 6. Cryptographic integrity check
        let computed_hash = format!("{:x}", hasher.finalize());
        if let Some(expected_hash) = expected_sha256 {
            if !computed_hash.eq_ignore_ascii_case(expected_hash) {
                // Remove corrupted file
                let _ = tokio::fs::remove_file(&part_path).await;
                return Err(AnnoraError::CryptoError(format!(
                    "SHA-256 verification failed for {}. Expected {}, got {}",
                    filename, expected_hash, computed_hash
                )));
            }
        }

        // Check file size if specified
        if let Some(exp_size) = expected_size {
            if downloaded_so_far != exp_size {
                tracing::warn!(
                    file = %filename,
                    expected = exp_size,
                    actual = downloaded_so_far,
                    "Downloaded size differs from catalog specification"
                );
            }
        }

        // 7. Atomic rename from .part to final destination
        tokio::fs::rename(&part_path, destination)
            .await
            .map_err(|e| AnnoraError::StorageError(format!(
                "Failed to rename {} to {}: {}",
                part_path.display(),
                destination.display(),
                e
            )))?;

        tracing::info!(
            file = %filename,
            size_bytes = downloaded_so_far,
            sha256 = %computed_hash,
            "Model artifact successfully downloaded, verified, and promoted to final path"
        );

        Ok(destination.to_path_buf())
    }

    /// Read an existing file and verify its SHA-256 checksum.
    pub fn verify_sha256(path: &Path, expected_hex: &str) -> Result<bool> {
        let mut file = std::fs::File::open(path)
            .map_err(|e| AnnoraError::StorageError(format!("Failed to open file for SHA-256 verification: {}", e)))?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];

        use std::io::Read;
        loop {
            let bytes_read = file.read(&mut buffer)
                .map_err(|e| AnnoraError::StorageError(format!("Failed reading file during checksum: {}", e)))?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        let computed = format!("{:x}", hasher.finalize());
        Ok(computed.eq_ignore_ascii_case(expected_hex))
    }

    /// Compute full SHA-256 of any file on disk.
    pub fn compute_sha256(path: &Path) -> Result<String> {
        let mut file = std::fs::File::open(path)
            .map_err(|e| AnnoraError::StorageError(format!("Failed to open file for SHA-256: {}", e)))?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];

        use std::io::Read;
        loop {
            let bytes_read = file.read(&mut buffer)
                .map_err(|e| AnnoraError::StorageError(format!("Failed reading file for SHA-256: {}", e)))?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    fn hash_file_partial(path: &Path, max_bytes: u64) -> Result<(u64, Sha256)> {
        let mut file = std::fs::File::open(path)
            .map_err(|e| AnnoraError::StorageError(format!("Failed to open file for partial hashing: {}", e)))?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];
        let mut total_read = 0u64;

        use std::io::Read;
        while total_read < max_bytes {
            let to_read = ((max_bytes - total_read) as usize).min(buffer.len());
            let bytes_read = file.read(&mut buffer[..to_read])
                .map_err(|e| AnnoraError::StorageError(format!("Failed reading partial file: {}", e)))?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
            total_read += bytes_read as u64;
        }

        Ok((total_read, hasher))
    }
}
