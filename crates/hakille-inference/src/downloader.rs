use futures_util::StreamExt;
use hakille_core::{HakilleError, Result};
use reqwest::header::{ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_RANGE, RANGE};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

/// Progression du transfert ; 100 % signifie que l'intégrité et la publication ont réussi.
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub filename: String,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
    pub speed_bytes_per_sec: f64,
    pub percentage: Option<f64>,
}

pub type ProgressCallback = Arc<dyn Fn(&DownloadProgress) + Send + Sync>;

/// Téléchargement vérifié, reprise bornée et publication atomique.
/// Les répertoires parents doivent être contrôlés par l'utilisateur.
#[derive(Clone)]
pub struct ModelDownloader {
    client: std::result::Result<reqwest::Client, String>,
}

impl Default for ModelDownloader {
    fn default() -> Self {
        Self::new()
    }
}

const MAX_ATTEMPTS: usize = 3;
const UNKNOWN_SIZE_LIMIT: u64 = 8 * 1024 * 1024 * 1024;

fn storage(error: impl std::fmt::Display) -> HakilleError {
    HakilleError::StorageError(error.to_string())
}

fn invalid(message: impl Into<String>) -> HakilleError {
    HakilleError::InferenceError(message.into())
}

/// Refuse liens symboliques, répertoires et fichiers spéciaux.
fn regular_file_size(path: &Path) -> Result<Option<u64>> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_file() => Ok(Some(meta.len())),
        Ok(_) => Err(storage(format!(
            "Artefact non régulier refusé : {}",
            path.display()
        ))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(storage(e)),
    }
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

/// Un verrou OS persistant (son inode ne doit pas être supprimé après usage).
/// Le descripteur libère automatiquement le verrou même après annulation/crash.
pub(crate) fn acquire_download_lock(destination: &Path) -> Result<std::fs::File> {
    let lock_path = append_suffix(destination, ".download.lock");
    regular_file_size(&lock_path)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(storage)?;
    file.try_lock().map_err(|e| {
        invalid(format!(
            "Téléchargement déjà actif ou verrou indisponible pour {} : {e}",
            destination.display()
        ))
    })?;
    Ok(file)
}

fn content_range(value: &str) -> Option<(u64, u64, u64)> {
    let (range, total) = value.strip_prefix("bytes ")?.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let (start, end, total) = (start.parse().ok()?, end.parse().ok()?, total.parse().ok()?);
    (start <= end && end < total).then_some((start, end, total))
}

impl ModelDownloader {
    #[cfg(test)]
    pub(crate) fn local_test_client() -> Self {
        Self {
            client: Ok(reqwest::Client::builder().no_proxy().build().unwrap()),
        }
    }

    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("Hakille-Cognitive-OS/0.1.0")
                .redirect(reqwest::redirect::Policy::limited(10))
                .connect_timeout(Duration::from_secs(20))
                .read_timeout(Duration::from_secs(60))
                .build()
                .map_err(|e| e.to_string()),
        }
    }

    /// Vérifie aussi les fichiers déjà présents. Un cache corrompu reste en place
    /// jusqu'à ce qu'une nouvelle copie vérifiée puisse le remplacer atomiquement.
    /// Les reprises sont permises seulement avec un SHA-256 attendu, afin de ne
    /// jamais assembler silencieusement deux versions différentes d'un artefact.
    pub async fn download_file(
        &self,
        url: &str,
        destination: &Path,
        expected_size: Option<u64>,
        expected_sha256: Option<&str>,
        progress: Option<ProgressCallback>,
    ) -> Result<PathBuf> {
        if let Some(hash) = expected_sha256
            && (hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(invalid("SHA-256 attendu invalide"));
        }
        if expected_size == Some(0) {
            return Err(invalid(
                "Un artefact modèle ne peut pas avoir une taille nulle",
            ));
        }
        let parsed = reqwest::Url::parse(url).map_err(|_| invalid("URL d'artefact invalide"))?;
        if !matches!(parsed.scheme(), "https" | "http")
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(invalid(
                "URL d'artefact : HTTP(S) sans identifiants attendu",
            ));
        }
        let client = self
            .client
            .as_ref()
            .map_err(|e| invalid(format!("Client HTTP indisponible : {e}")))?;
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        tokio::fs::create_dir_all(parent).await.map_err(storage)?;
        let _lock = acquire_download_lock(destination)?;
        let filename = destination
            .file_name()
            .ok_or_else(|| invalid("Nom d'artefact absent"))?
            .to_string_lossy()
            .into_owned();
        if Self::verify_file(destination, expected_size, expected_sha256)? {
            return Ok(destination.to_path_buf());
        }
        let part_path = append_suffix(destination, ".part");
        let mut last_error = invalid("Téléchargement incomplet");
        for attempt in 0..MAX_ATTEMPTS {
            let partial_size = regular_file_size(&part_path)?.unwrap_or(0);
            if partial_size > 0
                && expected_sha256.is_some()
                && Self::verify_file(&part_path, expected_size, expected_sha256)?
            {
                tokio::fs::rename(&part_path, destination)
                    .await
                    .map_err(storage)?;
                return Ok(destination.to_path_buf());
            }
            let can_resume = partial_size > 0
                && expected_sha256.is_some()
                && expected_size
                    .map(|size| partial_size < size)
                    .unwrap_or(partial_size < UNKNOWN_SIZE_LIMIT);
            let offset = if can_resume { partial_size } else { 0 };
            let (_, mut hasher) = if offset > 0 {
                Self::hash_file_partial(&part_path, offset)?
            } else {
                (0, Sha256::new())
            };
            let mut request = client
                .get(parsed.clone())
                .header(ACCEPT_ENCODING, "identity");
            if offset > 0 {
                request = request.header(RANGE, format!("bytes={offset}-"));
            }
            let response = match request.send().await {
                Ok(response) => response,
                Err(e) => {
                    last_error = invalid(format!(
                        "Connexion impossible pour {filename} : {}",
                        e.without_url()
                    ));
                    if attempt + 1 < MAX_ATTEMPTS {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                    continue;
                }
            };
            let status = response.status();
            if !(status == reqwest::StatusCode::OK
                || status == reqwest::StatusCode::PARTIAL_CONTENT)
            {
                last_error = invalid(format!("Téléchargement {filename} refusé : HTTP {status}"));
                if status.is_server_error() || matches!(status.as_u16(), 408 | 429) {
                    if attempt + 1 < MAX_ATTEMPTS {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                    continue;
                }
                // Un serveur peut rejeter une ancienne reprise : repartir de zéro
                // sans supprimer le fichier final, qui peut toujours être utilisé.
                if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE && offset > 0 {
                    tokio::fs::remove_file(&part_path).await.map_err(storage)?;
                    continue;
                }
                return Err(last_error);
            }
            if response
                .headers()
                .get(CONTENT_ENCODING)
                .is_some_and(|v| v.as_bytes() != b"identity")
            {
                return Err(invalid(
                    "Encodage HTTP compressé refusé pour un artefact binaire",
                ));
            }
            let declared = match response.headers().get(CONTENT_LENGTH) {
                Some(value) => Some(
                    value
                        .to_str()
                        .ok()
                        .and_then(|v| v.parse::<u64>().ok())
                        .ok_or_else(|| invalid("Content-Length invalide"))?,
                ),
                None => None,
            };
            let (base, total, response_end) = if status == reqwest::StatusCode::PARTIAL_CONTENT {
                let (start, end, total) = response
                    .headers()
                    .get(CONTENT_RANGE)
                    .and_then(|v| v.to_str().ok())
                    .and_then(content_range)
                    .ok_or_else(|| {
                        invalid("Reprise HTTP refusée : Content-Range absent ou invalide")
                    })?;
                if offset == 0
                    || start != offset
                    || expected_size.is_some_and(|size| size != total)
                    || declared.is_some_and(|length| length != end - start + 1)
                {
                    return Err(invalid(
                        "Reprise HTTP incohérente : offset, taille ou intervalle incorrect",
                    ));
                }
                (offset, Some(total), Some(end + 1))
            } else {
                if expected_size
                    .zip(declared)
                    .is_some_and(|(expected, actual)| expected != actual)
                {
                    return Err(invalid(
                        "Content-Length incompatible avec la taille du catalogue",
                    ));
                }
                hasher = Sha256::new();
                (0, expected_size.or(declared), expected_size.or(declared))
            };
            let limit = expected_size.unwrap_or(UNKNOWN_SIZE_LIMIT);
            if total.is_some_and(|total| total > limit) {
                return Err(invalid(
                    "Taille annoncée supérieure au budget de téléchargement",
                ));
            }
            let mut file = tokio::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .append(base > 0)
                .truncate(base == 0)
                .open(&part_path)
                .await
                .map_err(storage)?;
            let mut downloaded = base;
            let started = Instant::now();
            let mut last_progress = Instant::now();
            let mut interrupted = false;
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(e) => {
                        last_error = invalid(format!(
                            "Transfert interrompu pour {filename} : {}",
                            e.without_url()
                        ));
                        interrupted = true;
                        break;
                    }
                };
                let next = downloaded
                    .checked_add(chunk.len() as u64)
                    .ok_or_else(|| invalid("Taille de transfert débordée"))?;
                if next > limit || response_end.is_some_and(|end| next > end) {
                    drop(file);
                    tokio::fs::remove_file(&part_path).await.map_err(storage)?;
                    return Err(invalid(
                        "Réponse trop volumineuse : limite de l'artefact dépassée",
                    ));
                }
                file.write_all(&chunk).await.map_err(storage)?;
                hasher.update(&chunk);
                downloaded = next;
                if let Some(callback) = &progress
                    && last_progress.elapsed() >= Duration::from_millis(100)
                {
                    callback(&DownloadProgress {
                        filename: filename.clone(),
                        bytes_downloaded: downloaded,
                        total_bytes: total,
                        speed_bytes_per_sec: (downloaded - base) as f64
                            / started.elapsed().as_secs_f64().max(0.001),
                        percentage: total
                            .filter(|t| *t > 0)
                            .map(|t| (downloaded as f64 * 100.0 / t as f64).min(99.9)),
                    });
                    last_progress = Instant::now();
                }
            }
            file.flush().await.map_err(storage)?;
            file.sync_all().await.map_err(storage)?;
            drop(file);
            if interrupted {
                continue;
            }
            if downloaded == 0
                || total.is_some_and(|total| downloaded != total)
                || response_end.is_some_and(|end| downloaded != end)
            {
                last_error = invalid(format!(
                    "Transfert incomplet pour {filename} : {downloaded} octets"
                ));
                continue;
            }
            let actual_hash = format!("{:x}", hasher.finalize());
            if expected_sha256.is_some_and(|hash| !hash.eq_ignore_ascii_case(&actual_hash)) {
                tokio::fs::remove_file(&part_path).await.map_err(storage)?;
                last_error =
                    HakilleError::CryptoError(format!("SHA-256 incorrect pour {filename}"));
                continue;
            }
            tokio::fs::rename(&part_path, destination)
                .await
                .map_err(storage)?;
            if let Some(callback) = &progress {
                callback(&DownloadProgress {
                    filename,
                    bytes_downloaded: downloaded,
                    total_bytes: Some(downloaded),
                    speed_bytes_per_sec: (downloaded - base) as f64
                        / started.elapsed().as_secs_f64().max(0.001),
                    percentage: Some(100.0),
                });
            }
            return Ok(destination.to_path_buf());
        }
        Err(last_error)
    }

    /// Présence régulière, taille exacte et checksum s'il est fourni.
    /// Sans checksum, ce contrôle n'est pas une preuve cryptographique.
    pub fn verify_file(
        path: &Path,
        expected_size: Option<u64>,
        expected_sha256: Option<&str>,
    ) -> Result<bool> {
        let Some(size) = regular_file_size(path)? else {
            return Ok(false);
        };
        if size == 0 || expected_size.is_some_and(|expected| expected != size) {
            return Ok(false);
        }
        match expected_sha256 {
            Some(hash) => Self::verify_sha256(path, hash),
            None => Ok(true),
        }
    }

    pub fn verify_sha256(path: &Path, expected_hex: &str) -> Result<bool> {
        Ok(Self::compute_sha256(path)?.eq_ignore_ascii_case(expected_hex))
    }

    pub fn compute_sha256(path: &Path) -> Result<String> {
        let size = regular_file_size(path)?.ok_or_else(|| storage("Artefact absent"))?;
        let (read, hash) = Self::hash_file_partial(path, size)?;
        if read != size {
            return Err(storage("Artefact modifié pendant son calcul SHA-256"));
        }
        Ok(format!("{:x}", hash.finalize()))
    }

    fn hash_file_partial(path: &Path, max_bytes: u64) -> Result<(u64, Sha256)> {
        use std::io::Read;
        let mut file = std::fs::File::open(path).map_err(storage)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];
        let mut total = 0;
        while total < max_bytes {
            let count = ((max_bytes - total).min(buffer.len() as u64)) as usize;
            let read = file.read(&mut buffer[..count]).map_err(storage)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            total += read as u64;
        }
        Ok((total, hasher))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    pub(super) struct TestDir(pub PathBuf);
    impl TestDir {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("hakille-download-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn hash(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }
    fn response(status: &str, headers: &str, body: &[u8]) -> Vec<u8> {
        let mut bytes =
            format!("HTTP/1.1 {status}\r\nConnection: close\r\n{headers}\r\n").into_bytes();
        bytes.extend_from_slice(body);
        bytes
    }
    async fn server(responses: Vec<Vec<u8>>) -> (String, tokio::task::JoinHandle<Vec<String>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/artifact", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for response in responses {
                let (mut socket, _) =
                    tokio::time::timeout(Duration::from_secs(5), listener.accept())
                        .await
                        .unwrap()
                        .unwrap();
                let mut request = Vec::new();
                let mut chunk = [0u8; 1024];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    let read = socket.read(&mut chunk).await.unwrap();
                    assert!(read > 0 && request.len() < 16384);
                    request.extend_from_slice(&chunk[..read]);
                }
                requests.push(String::from_utf8(request).unwrap().to_lowercase());
                // Un refus anticipé côté client peut fermer avant toute la réponse.
                let _ = socket.write_all(&response).await;
            }
            requests
        });
        (url, task)
    }
    fn downloader() -> ModelDownloader {
        // Les tests locaux ne dépendent pas d'une configuration proxy de la machine.
        ModelDownloader {
            client: Ok(reqwest::Client::builder().no_proxy().build().unwrap()),
        }
    }

    #[tokio::test]
    async fn corrupted_cache_is_repaired_and_valid_cache_skips_network() {
        let dir = TestDir::new();
        let path = dir.0.join("model.onnx");
        std::fs::write(&path, b"bad-data").unwrap();
        let body = b"good-one";
        let (url, server) = server(vec![response("200 OK", "Content-Length: 8\r\n", body)]).await;
        downloader()
            .download_file(&url, &path, Some(8), Some(&hash(body)), None)
            .await
            .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), body);
        assert_eq!(server.await.unwrap().len(), 1);
        downloader()
            .download_file(
                "http://127.0.0.1:1/offline",
                &path,
                Some(8),
                Some(&hash(body)),
                None,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn valid_partial_content_resumes_at_exact_offset() {
        let dir = TestDir::new();
        let path = dir.0.join("model.onnx");
        std::fs::write(append_suffix(&path, ".part"), b"abcd").unwrap();
        let (url, server) = server(vec![response(
            "206 Partial Content",
            "Content-Range: bytes 4-7/8\r\nContent-Length: 4\r\n",
            b"efgh",
        )])
        .await;
        downloader()
            .download_file(&url, &path, Some(8), Some(&hash(b"abcdefgh")), None)
            .await
            .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"abcdefgh");
        assert!(server.await.unwrap()[0].contains("range: bytes=4-"));
    }

    #[tokio::test]
    async fn ignored_range_restarts_instead_of_appending() {
        let dir = TestDir::new();
        let path = dir.0.join("model.onnx");
        std::fs::write(append_suffix(&path, ".part"), b"abcd").unwrap();
        let (url, server) = server(vec![response(
            "200 OK",
            "Content-Length: 8\r\n",
            b"abcdefgh",
        )])
        .await;
        let progress = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = progress.clone();
        downloader()
            .download_file(
                &url,
                &path,
                Some(8),
                Some(&hash(b"abcdefgh")),
                Some(Arc::new(move |p| recorded.lock().unwrap().push(p.clone()))),
            )
            .await
            .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"abcdefgh");
        {
            let records = progress.lock().unwrap();
            assert_eq!(records.last().unwrap().percentage, Some(100.0));
            assert!(records.iter().all(|p| p.speed_bytes_per_sec.is_finite()));
        }
        server.await.unwrap();
    }

    #[tokio::test]
    async fn interrupted_response_resumes_and_complete_corrupt_partial_restarts() {
        let dir = TestDir::new();
        let path = dir.0.join("model.onnx");
        let (url, task) = server(vec![
            response("200 OK", "Content-Length: 8\r\n", b"abcd"),
            response(
                "206 Partial Content",
                "Content-Range: bytes 4-7/8\r\nContent-Length: 4\r\n",
                b"efgh",
            ),
        ])
        .await;
        downloader()
            .download_file(&url, &path, Some(8), Some(&hash(b"abcdefgh")), None)
            .await
            .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"abcdefgh");
        assert!(task.await.unwrap()[1].contains("range: bytes=4-"));
        std::fs::remove_file(&path).unwrap();
        std::fs::write(append_suffix(&path, ".part"), b"corrupt!").unwrap();
        let (url, task) = server(vec![response(
            "200 OK",
            "Content-Length: 8\r\n",
            b"abcdefgh",
        )])
        .await;
        downloader()
            .download_file(&url, &path, Some(8), Some(&hash(b"abcdefgh")), None)
            .await
            .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"abcdefgh");
        assert!(!task.await.unwrap()[0].contains("range:"));
    }

    #[tokio::test]
    async fn invalid_content_range_never_appends_or_promotes() {
        for header in [
            "",
            "Content-Range: bytes 0-3/8\r\n",
            "Content-Range: bytes 4-7/9\r\n",
            "Content-Range: bytes 4-8/8\r\n",
        ] {
            let dir = TestDir::new();
            let path = dir.0.join("model.onnx");
            let part = append_suffix(&path, ".part");
            std::fs::write(&part, b"abcd").unwrap();
            let (url, server) = server(vec![response(
                "206 Partial Content",
                &format!("{header}Content-Length: 4\r\n"),
                b"efgh",
            )])
            .await;
            assert!(
                downloader()
                    .download_file(&url, &path, Some(8), Some(&hash(b"abcdefgh")), None)
                    .await
                    .is_err()
            );
            assert!(!path.exists());
            assert_eq!(std::fs::read(part).unwrap(), b"abcd");
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn bad_checksum_never_reports_completion_or_replaces_final() {
        let dir = TestDir::new();
        let path = dir.0.join("model.onnx");
        std::fs::write(&path, b"previous").unwrap();
        let (url, server) = server(vec![
            response("200 OK", "Content-Length: 8\r\n", b"bad-body");
            MAX_ATTEMPTS
        ])
        .await;
        let completed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let seen = completed.clone();
        let result = downloader()
            .download_file(
                &url,
                &path,
                Some(8),
                Some(&hash(b"good-one")),
                Some(Arc::new(move |p| {
                    if p.percentage == Some(100.0) {
                        seen.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                })),
            )
            .await;
        assert!(matches!(result, Err(HakilleError::CryptoError(_))));
        assert!(!completed.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(std::fs::read(&path).unwrap(), b"previous");
        assert!(!append_suffix(&path, ".part").exists());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn oversized_stream_is_rejected_before_publication() {
        let dir = TestDir::new();
        let path = dir.0.join("model.onnx");
        let (url, server) = server(vec![response(
            "200 OK",
            "Transfer-Encoding: chunked\r\n",
            b"9\r\n123456789\r\n0\r\n\r\n",
        )])
        .await;
        assert!(
            downloader()
                .download_file(&url, &path, Some(8), None, None)
                .await
                .is_err()
        );
        assert!(!path.exists());
        assert!(!append_suffix(&path, ".part").exists());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn concurrent_writer_is_refused_without_touching_partial() {
        let dir = TestDir::new();
        let path = dir.0.join("model.onnx");
        let part = append_suffix(&path, ".part");
        std::fs::write(&part, b"abcd").unwrap();
        let _lock = acquire_download_lock(&path).unwrap();
        let error = downloader()
            .download_file(
                "http://127.0.0.1:1/unused",
                &path,
                Some(8),
                Some(&hash(b"abcdefgh")),
                None,
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("verrou"));
        assert_eq!(std::fs::read(part).unwrap(), b"abcd");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlink_destination_and_partial_are_refused() {
        let dir = TestDir::new();
        let victim = dir.0.join("victim");
        std::fs::write(&victim, b"keep").unwrap();
        for suffix in ["", ".part", ".download.lock"] {
            let path = dir.0.join(format!("model-{}", uuid::Uuid::new_v4()));
            std::os::unix::fs::symlink(&victim, append_suffix(&path, suffix)).unwrap();
            assert!(
                downloader()
                    .download_file(
                        "http://127.0.0.1:1/unused",
                        &path,
                        Some(8),
                        Some(&hash(b"abcdefgh")),
                        None
                    )
                    .await
                    .is_err()
            );
            assert_eq!(std::fs::read(&victim).unwrap(), b"keep");
        }
    }
}
