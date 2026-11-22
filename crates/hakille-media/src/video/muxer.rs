//! Muxer vidéo explicite via `ffmpeg` (`MP4 H.264 + AAC`, `WebM VP9 + Opus`).
//!
//! Sécurité & contrat d'honnêteté :
//! - Invoque uniquement `Command::new(binary)` (jamais de shell `sh -c`) ;
//! - Détection explicite via `HAKILLE_FFMPEG_BIN` ou `PATH` ;
//! - Si `ffmpeg` est absent, renvoie [`MediaError::MuxerUnavailable`] sans
//!   jamais masquer l'échec ni convertir silencieusement en un autre format.

use crate::error::{MediaError, MediaResult};
use crate::image::encode::RgbaImageBuf;
use crate::types::VideoContainer;
use crate::video::encoder::encode_y4m;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Pilote explicite du binaire `ffmpeg` pour muxer vidéo + piste audio WAV.
#[derive(Debug, Clone)]
pub struct FfmpegMuxer {
    binary: PathBuf,
}

impl FfmpegMuxer {
    /// Détecte `ffmpeg` (`HAKILLE_FFMPEG_BIN` prioritaire, puis `ffmpeg` sur `PATH`).
    pub fn detect() -> Option<Self> {
        if let Ok(explicit) = std::env::var("HAKILLE_FFMPEG_BIN") {
            let p = PathBuf::from(explicit.trim());
            if p.is_file() && Self::probe_binary(&p) {
                return Some(Self { binary: p });
            }
            return None;
        }
        let default = PathBuf::from("ffmpeg");
        if Self::probe_binary(&default) {
            Some(Self { binary: default })
        } else {
            None
        }
    }

    pub fn binary_path(&self) -> &Path {
        &self.binary
    }

    fn probe_binary(path: &Path) -> bool {
        Command::new(path)
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Encode une séquence de frames RGBA8 (et éventuellement une piste WAV)
    /// en conteneur MP4 (H.264 `yuv420p` + AAC) ou WebM (VP9 + Opus).
    pub fn mux_frames(
        &self,
        frames: &[RgbaImageBuf],
        fps: u16,
        container: VideoContainer,
        audio_wav_path: Option<&Path>,
    ) -> MediaResult<Vec<u8>> {
        if !matches!(container, VideoContainer::Mp4H264 | VideoContainer::WebmVp9) {
            return Err(MediaError::InvalidRequest(format!(
                "FfmpegMuxer ne s'applique qu'aux conteneurs Mp4H264 / WebmVp9 (reçu {container:?})"
            )));
        }
        let y4m_bytes = encode_y4m(frames, fps)?;
        let tmp_dir = tempfile::tempdir()?;
        let y4m_path = tmp_dir.path().join("input.y4m");
        std::fs::write(&y4m_path, &y4m_bytes)?;

        let out_path = tmp_dir
            .path()
            .join(format!("output.{}", container.extension()));

        self.mux_y4m_file(&y4m_path, audio_wav_path, &out_path, container)?;
        let bytes = std::fs::read(&out_path)?;
        if bytes.is_empty() {
            return Err(MediaError::Encoding(
                "ffmpeg a produit un fichier de sortie vide".into(),
            ));
        }
        Ok(bytes)
    }

    /// Mux un fichier vidéo existant (Y4M, APNG, MP4, WebM) avec une piste audio WAV.
    pub fn mux_files(
        &self,
        video_path: &Path,
        audio_wav_path: Option<&Path>,
        container: VideoContainer,
    ) -> MediaResult<Vec<u8>> {
        let tmp_dir = tempfile::tempdir()?;
        let out_path = tmp_dir
            .path()
            .join(format!("muxed.{}", container.extension()));
        self.mux_y4m_file(video_path, audio_wav_path, &out_path, container)?;
        let bytes = std::fs::read(&out_path)?;
        if bytes.is_empty() {
            return Err(MediaError::Encoding(
                "ffmpeg a produit un fichier muxé vide".into(),
            ));
        }
        Ok(bytes)
    }

    fn mux_y4m_file(
        &self,
        video_in: &Path,
        audio_in: Option<&Path>,
        output: &Path,
        container: VideoContainer,
    ) -> MediaResult<()> {
        // Canonicalise les entrées locales : empêche l'injection d'options
        // (`-foo`) ou de schémas réseau (`http://...`) dans les arguments `-i`.
        let video_canon = std::fs::canonicalize(video_in)?;
        if !video_canon.is_file() {
            return Err(MediaError::InvalidRequest(
                "entrée vidéo ffmpeg introuvable ou non régulière".into(),
            ));
        }
        let audio_canon = match audio_in {
            Some(a) => {
                let c = std::fs::canonicalize(a)?;
                if !c.is_file() {
                    return Err(MediaError::InvalidRequest(
                        "entrée audio ffmpeg introuvable ou non régulière".into(),
                    ));
                }
                Some(c)
            }
            None => None,
        };

        let mut cmd = Command::new(&self.binary);
        cmd.arg("-nostdin")
            .arg("-y")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-i")
            .arg(&video_canon);

        if let Some(audio) = &audio_canon {
            cmd.arg("-i").arg(audio);
        }

        match container {
            VideoContainer::Mp4H264 => {
                cmd.args([
                    "-c:v",
                    "libx264",
                    "-pix_fmt",
                    "yuv420p",
                    "-preset",
                    "veryfast",
                    "-crf",
                    "22",
                    "-movflags",
                    "+faststart",
                ]);
                if audio_in.is_some() {
                    cmd.args(["-c:a", "aac", "-b:a", "128k", "-shortest"]);
                }
            }
            VideoContainer::WebmVp9 => {
                cmd.args([
                    "-c:v",
                    "libvpx-vp9",
                    "-pix_fmt",
                    "yuv420p",
                    "-b:v",
                    "0",
                    "-crf",
                    "32",
                    "-deadline",
                    "realtime",
                    "-cpu-used",
                    "6",
                ]);
                if audio_in.is_some() {
                    cmd.args(["-c:a", "libopus", "-b:a", "96k", "-shortest"]);
                }
            }
            _ => {
                return Err(MediaError::InvalidRequest(
                    "conteneur non géré par FfmpegMuxer".into(),
                ));
            }
        }

        cmd.arg(output);

        let out = cmd.output().map_err(|e| MediaError::MuxerUnavailable {
            binary: self.binary.display().to_string(),
            reason: format!("exécution ffmpeg impossible : {e}"),
        })?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(MediaError::Encoding(format!(
                "ffmpeg a échoué (code {:?}) : {stderr}",
                out.status.code()
            )));
        }
        Ok(())
    }
}
