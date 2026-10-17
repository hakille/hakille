use annora_core::{AnnoraError, Result};
use futures_util::StreamExt;
use reqwest::header::{CONTENT_LENGTH, RANGE};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
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
    /// - Resumes partial downloads if supported by upstream server with up to 10 automatic retries.
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
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                AnnoraError::StorageError(format!(
                    "Failed to create directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        // Check for existing partial file either as .part extension or appended .part
        let part_path = if destination.with_extension("part").exists() {
            destination.with_extension("part")
        } else {
            PathBuf::from(format!("{}.part", destination.display()))
        };

        let max_retries = 10;
        let mut attempt = 0;

        loop {
            attempt += 1;

            // 2. Check partial file size and calculate partial hash for resume
            let mut existing_bytes: u64 = 0;
            let mut hasher = Sha256::new();

            if part_path.exists() {
                if let Ok(metadata) = std::fs::metadata(&part_path) {
                    let size = metadata.len();
                    if let Some(exp) = expected_size {
                        if size == exp {
                            // File already finished downloading in previous attempts
                            existing_bytes = size;
                            if let Ok((bytes_read, pre_hasher)) =
                                Self::hash_file_partial(&part_path, size)
                            {
                                if bytes_read == size {
                                    hasher = pre_hasher;
                                    let computed_hash = format!("{:x}", hasher.finalize());
                                    if let Some(expected_hash) = expected_sha256 {
                                        if !computed_hash.eq_ignore_ascii_case(expected_hash) {
                                            let _ = tokio::fs::remove_file(&part_path).await;
                                            return Err(AnnoraError::CryptoError(format!(
                                                "SHA-256 verification failed for {}. Expected {}, got {}",
                                                filename, expected_hash, computed_hash
                                            )));
                                        }
                                    }
                                    tokio::fs::rename(&part_path, destination).await.map_err(
                                        |e| {
                                            AnnoraError::StorageError(format!(
                                                "Failed to promote part file: {}",
                                                e
                                            ))
                                        },
                                    )?;
                                    return Ok(destination.to_path_buf());
                                }
                            }
                        }
                    }

                    if size > 0 && expected_size.map(|exp| size < exp).unwrap_or(true) {
                        tracing::info!(file = %filename, bytes = size, attempt = attempt, "Resuming partial model artifact download");
                        match Self::hash_file_partial(&part_path, size) {
                            Ok((bytes_read, pre_hasher)) if bytes_read == size => {
                                existing_bytes = size;
                                hasher = pre_hasher;
                            }
                            _ => {
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

            let response = match request.send().await {
                Ok(resp) => resp,
                Err(e) => {
                    if attempt >= max_retries {
                        return Err(AnnoraError::InferenceError(format!(
                            "Network download failed after {} attempts for {}: {}",
                            attempt, url, e
                        )));
                    }
                    tracing::warn!(attempt = attempt, error = %e, "Download connection error, retrying in 3s...");
                    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                    continue;
                }
            };

            let status = response.status();
            let (mut file, mut downloaded_so_far, total_bytes) =
                if existing_bytes > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT {
                    // Resume accepted
                    let content_len = response
                        .headers()
                        .get(CONTENT_LENGTH)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok());
                    let total = content_len.map(|cl| cl + existing_bytes).or(expected_size);

                    let f = tokio::fs::OpenOptions::new()
                        .write(true)
                        .append(true)
                        .open(&part_path)
                        .await
                        .map_err(|e| {
                            AnnoraError::StorageError(format!(
                                "Failed to open .part file for append: {}",
                                e
                            ))
                        })?;

                    (f, existing_bytes, total)
                } else {
                    // Fresh download or server does not support Range
                    if !status.is_success() {
                        if attempt >= max_retries {
                            return Err(AnnoraError::InferenceError(format!(
                                "HTTP request failed with status code {} for URL: {}",
                                status, url
                            )));
                        }
                        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                        continue;
                    }

                    hasher = Sha256::new();
                    let total = response
                        .headers()
                        .get(CONTENT_LENGTH)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .or(expected_size);

                    let f = tokio::fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(&part_path)
                        .await
                        .map_err(|e| {
                            AnnoraError::StorageError(format!("Failed to create .part file: {}", e))
                        })?;

                    (f, 0u64, total)
                };

            // 4. Stream response body
            let mut stream = response.bytes_stream();
            let start_time = Instant::now();
            let mut last_progress_time = Instant::now();
            let mut stream_interrupted = false;

            while let Some(chunk_res) = stream.next().await {
                match chunk_res {
                    Ok(chunk) => {
                        if let Err(e) = file.write_all(&chunk).await {
                            return Err(AnnoraError::StorageError(format!(
                                "Failed to write chunk to disk: {}",
                                e
                            )));
                        }
                        hasher.update(&chunk);
                        downloaded_so_far += chunk.len() as u64;

                        // Notify progress throttled to 100ms
                        if let Some(ref cb) = progress {
                            if last_progress_time.elapsed().as_millis() >= 100 {
                                let elapsed = start_time.elapsed().as_secs_f64().max(0.001);
                                let speed = (downloaded_so_far - existing_bytes) as f64 / elapsed;
                                let percentage = total_bytes
                                    .map(|t| (downloaded_so_far as f64 / t as f64) * 100.0);

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
                    Err(e) => {
                        tracing::warn!(
                            attempt = attempt,
                            downloaded = downloaded_so_far,
                            error = %e,
                            "Stream chunk read interrupted, will auto-resume"
                        );
                        stream_interrupted = true;
                        break;
                    }
                }
            }

            // Flush and close file
            let _ = file.flush().await;
            drop(file);

            if stream_interrupted {
                if attempt >= max_retries {
                    return Err(AnnoraError::InferenceError(format!(
                        "Download stream for {} failed after {} attempts",
                        filename, attempt
                    )));
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                continue;
            }

            // If expected size is known and not reached, loop to resume
            if let Some(exp_size) = expected_size {
                if downloaded_so_far < exp_size {
                    tracing::warn!(
                        downloaded = downloaded_so_far,
                        expected = exp_size,
                        "Connection closed before reaching expected size, resuming..."
                    );
                    if attempt >= max_retries {
                        return Err(AnnoraError::InferenceError(format!(
                            "Incomplete download for {}: expected {} bytes, got {}",
                            filename, exp_size, downloaded_so_far
                        )));
                    }
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    continue;
                }
            }

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
                    let _ = tokio::fs::remove_file(&part_path).await;
                    return Err(AnnoraError::CryptoError(format!(
                        "SHA-256 verification failed for {}. Expected {}, got {}",
                        filename, expected_hash, computed_hash
                    )));
                }
            }

            // 7. Atomic rename from .part to final destination
            tokio::fs::rename(&part_path, destination)
                .await
                .map_err(|e| {
                    AnnoraError::StorageError(format!(
                        "Failed to rename {} to {}: {}",
                        part_path.display(),
                        destination.display(),
                        e
                    ))
                })?;

            tracing::info!(
                file = %filename,
                size_bytes = downloaded_so_far,
                sha256 = %computed_hash,
                "Model artifact successfully downloaded, verified, and promoted to final path"
            );

            return Ok(destination.to_path_buf());
        }
    }

    /// Read an existing file and verify its SHA-256 checksum.
    pub fn verify_sha256(path: &Path, expected_hex: &str) -> Result<bool> {
        let mut file = std::fs::File::open(path).map_err(|e| {
            AnnoraError::StorageError(format!(
                "Failed to open file for SHA-256 verification: {}",
                e
            ))
        })?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];

        use std::io::Read;
        loop {
            let bytes_read = file.read(&mut buffer).map_err(|e| {
                AnnoraError::StorageError(format!("Failed reading file during checksum: {}", e))
            })?;
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
        let mut file = std::fs::File::open(path).map_err(|e| {
            AnnoraError::StorageError(format!("Failed to open file for SHA-256: {}", e))
        })?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];

        use std::io::Read;
        loop {
            let bytes_read = file.read(&mut buffer).map_err(|e| {
                AnnoraError::StorageError(format!("Failed reading file for SHA-256: {}", e))
            })?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    fn hash_file_partial(path: &Path, max_bytes: u64) -> Result<(u64, Sha256)> {
        let mut file = std::fs::File::open(path).map_err(|e| {
            AnnoraError::StorageError(format!("Failed to open file for partial hashing: {}", e))
        })?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];
        let mut total_read = 0u64;

        use std::io::Read;
        while total_read < max_bytes {
            let to_read = ((max_bytes - total_read) as usize).min(buffer.len());
            let bytes_read = file.read(&mut buffer[..to_read]).map_err(|e| {
                AnnoraError::StorageError(format!("Failed reading partial file: {}", e))
            })?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
            total_read += bytes_read as u64;
        }

        Ok((total_read, hasher))
    }
}
