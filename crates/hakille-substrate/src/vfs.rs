//! Confinement VirtualFS partagé par les substrats OS natifs (Linux, macOS, Windows).
//!
//! Garanties fail-closed :
//! - Rejet des octets NUL, préfixes UNC (`\\`), séparateurs `:` (lecteurs/ADS),
//!   noms réservés Windows (`CON`, `PRN`, `AUX`, `NUL`, `COM1..9`, `LPT1..9`)
//!   et composants non normaux (`..`, racine, préfixe).
//! - Vérification composant par composant contre toute évasion par lien symbolique
//!   (`symlink_metadata()` sur chaque ancêtre existant ET sur la feuille).
//! - Canonisation de la racine et de l'ancêtre existant le plus profond avec
//!   vérification stricte `starts_with(&canonical_root)`.
//! - Plafond strict de lecture et d'écriture (`VFS_MAX_FILE_BYTES`, 64 Mio)
//!   pour empêcher les dénis de service mémoire.

use hakille_core::{HakilleError, Result, VirtualPath};
use std::path::{Component, Path, PathBuf};
use tokio::io::AsyncReadExt;

/// Taille maximale d'un fichier lu ou écrit via le VirtualFS (64 Mio).
pub const VFS_MAX_FILE_BYTES: usize = 64 * 1024 * 1024;

/// Indique si un segment de chemin correspond à un nom de périphérique réservé Windows.
fn is_windows_reserved_name(name: &str) -> bool {
    let trimmed = name.trim_end_matches(['.', ' ']);
    let stem = trimmed.split('.').next().unwrap_or(trimmed);
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

/// Valide et normalise un [`VirtualPath`] en segments relatifs sûrs.
/// Retourne un vecteur vide si le chemin virtuel désigne la racine du VirtualFS.
pub fn validate_virtual_components(path: &VirtualPath) -> Result<Vec<String>> {
    if path.0.contains('\0') {
        return Err(HakilleError::CapabilityDenied(
            "NUL byte in virtual path".into(),
        ));
    }
    let lower = path.0.to_ascii_lowercase();
    if lower.contains(':') || path.0.starts_with(r"\\") {
        return Err(HakilleError::CapabilityDenied(
            "absolute / drive path rejected".into(),
        ));
    }
    // Normalise les deux séparateurs pour bloquer `..\evil` même sur Unix.
    let normalized = path.0.replace('\\', "/");
    let mut segments = Vec::new();
    for comp in Path::new(&normalized).components() {
        match comp {
            Component::Normal(p) => {
                let Some(s) = p.to_str() else {
                    return Err(HakilleError::CapabilityDenied(
                        "non-UTF-8 path component rejected".into(),
                    ));
                };
                if s == ".." {
                    return Err(HakilleError::CapabilityDenied(
                        "path traversal (..) rejected".into(),
                    ));
                }
                if is_windows_reserved_name(s) {
                    return Err(HakilleError::CapabilityDenied(format!(
                        "nom de périphérique réservé refusé : '{s}'"
                    )));
                }
                segments.push(s.to_string());
            }
            Component::CurDir => {}
            _ => {
                return Err(HakilleError::CapabilityDenied(
                    "non-normal path component rejected".into(),
                ));
            }
        }
    }
    Ok(segments)
}

/// Résout `path` sous `root` en refusant tout lien symbolique et toute sortie
/// hors de `root`.
///
/// Si `allow_missing_leaf` vaut `false`, le fichier/répertoire final (ou son
/// ancêtre) est vérifié contre la racine canonisée ; les liens symboliques sur
/// n'importe quel segment existant sont systématiquement refusés.
pub fn resolve_confined_path(
    root: &Path,
    path: &VirtualPath,
    allow_missing_leaf: bool,
) -> Result<PathBuf> {
    let segments = validate_virtual_components(path)?;

    let root_meta = std::fs::symlink_metadata(root).ok();
    if let Some(meta) = &root_meta
        && meta.file_type().is_symlink()
    {
        return Err(HakilleError::CapabilityDenied(
            "la racine VirtualFS ne peut pas être un lien symbolique".into(),
        ));
    }

    let root_canonical = if root.exists() {
        Some(root.canonicalize().map_err(|e| {
            HakilleError::StorageError(format!(
                "impossible de canoniser la racine VirtualFS {}: {e}",
                root.display()
            ))
        })?)
    } else {
        None
    };

    if segments.is_empty() {
        return Ok(root_canonical.unwrap_or_else(|| root.to_path_buf()));
    }

    let mut current = root_canonical.clone().unwrap_or_else(|| root.to_path_buf());
    for seg in &segments {
        current.push(seg);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err(HakilleError::CapabilityDenied(format!(
                        "lien symbolique refusé dans le VirtualFS : '{}'",
                        path.0
                    )));
                }
                if let Some(ref root_c) = root_canonical {
                    let cur_c = current.canonicalize().map_err(|e| {
                        HakilleError::StorageError(format!(
                            "impossible de canoniser {}: {e}",
                            current.display()
                        ))
                    })?;
                    if !cur_c.starts_with(root_c) {
                        return Err(HakilleError::CapabilityDenied(
                            "resolved path escapes VirtualFS root".into(),
                        ));
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if !allow_missing_leaf {
                    // Continue pour vérifier qu'aucun parent déjà existant n'était un symlink ;
                    // le fichier final absent sera signalé par l'appelant.
                    break;
                }
            }
            Err(e) => {
                return Err(HakilleError::StorageError(format!(
                    "erreur de métadonnées VirtualFS {}: {e}",
                    current.display()
                )));
            }
        }
    }

    let final_path = segments.iter().fold(
        root_canonical.clone().unwrap_or_else(|| root.to_path_buf()),
        |mut acc, seg| {
            acc.push(seg);
            acc
        },
    );

    if let (Some(root_c), Ok(final_c)) = (&root_canonical, final_path.canonicalize()) {
        if !final_c.starts_with(root_c) {
            return Err(HakilleError::CapabilityDenied(
                "resolved path escapes VirtualFS root".into(),
            ));
        }
        return Ok(final_c);
    }

    Ok(final_path)
}

/// Lecture confinée et bornée (`<= VFS_MAX_FILE_BYTES`) d'un fichier du VirtualFS.
pub async fn read_confined_file(root: &Path, path: &VirtualPath) -> Result<Vec<u8>> {
    let local_path = resolve_confined_path(root, path, false)?;
    let meta = match tokio::fs::symlink_metadata(&local_path).await {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(HakilleError::StorageError(format!(
                "File not found: {}",
                path.0
            )));
        }
        Err(e) => {
            return Err(HakilleError::StorageError(format!(
                "Read metadata error {}: {e}",
                local_path.display()
            )));
        }
    };
    if meta.file_type().is_symlink() {
        return Err(HakilleError::CapabilityDenied(
            "lien symbolique refusé lors de la lecture VirtualFS".into(),
        ));
    }
    if !meta.is_file() {
        return Err(HakilleError::StorageError(format!(
            "Not a regular file: {}",
            path.0
        )));
    }
    if meta.len() > VFS_MAX_FILE_BYTES as u64 {
        return Err(HakilleError::StorageError(format!(
            "fichier VirtualFS trop volumineux ({} octets > plafond {})",
            meta.len(),
            VFS_MAX_FILE_BYTES
        )));
    }
    let file = tokio::fs::File::open(&local_path).await.map_err(|e| {
        HakilleError::StorageError(format!("Read error {}: {e}", local_path.display()))
    })?;
    let mut buf = Vec::with_capacity((meta.len() as usize).min(VFS_MAX_FILE_BYTES));
    file.take((VFS_MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut buf)
        .await
        .map_err(|e| {
            HakilleError::StorageError(format!("Read error {}: {e}", local_path.display()))
        })?;
    if buf.len() > VFS_MAX_FILE_BYTES {
        return Err(HakilleError::StorageError(format!(
            "fichier VirtualFS dépasse le plafond de {} octets",
            VFS_MAX_FILE_BYTES
        )));
    }
    Ok(buf)
}

/// Écriture confinée et bornée (`<= VFS_MAX_FILE_BYTES`) dans le VirtualFS.
pub async fn write_confined_file(root: &Path, path: &VirtualPath, data: &[u8]) -> Result<()> {
    if data.len() > VFS_MAX_FILE_BYTES {
        return Err(HakilleError::StorageError(format!(
            "écriture VirtualFS refusée : {} octets > plafond {}",
            data.len(),
            VFS_MAX_FILE_BYTES
        )));
    }
    let segments = validate_virtual_components(path)?;
    if segments.is_empty() {
        return Err(HakilleError::CapabilityDenied(
            "impossible d'écrire directement sur la racine VirtualFS".into(),
        ));
    }

    // Crée la racine si absente puis vérifie qu'elle n'est pas un lien symbolique.
    if let Ok(meta) = tokio::fs::symlink_metadata(root).await {
        if meta.file_type().is_symlink() {
            return Err(HakilleError::CapabilityDenied(
                "la racine VirtualFS ne peut pas être un lien symbolique".into(),
            ));
        }
    } else {
        tokio::fs::create_dir_all(root).await.map_err(|e| {
            HakilleError::StorageError(format!("Create root dir error {}: {e}", root.display()))
        })?;
    }
    let root_canonical = root.canonicalize().map_err(|e| {
        HakilleError::StorageError(format!("Canonicalize root error {}: {e}", root.display()))
    })?;

    // Crée chaque sous-répertoire parent en vérifiant l'absence de lien symbolique.
    let mut cur = root_canonical.clone();
    for dir_seg in &segments[..segments.len() - 1] {
        cur.push(dir_seg);
        match tokio::fs::symlink_metadata(&cur).await {
            Ok(meta) => {
                if meta.file_type().is_symlink() || !meta.is_dir() {
                    return Err(HakilleError::CapabilityDenied(format!(
                        "segment parent invalide ou lien symbolique dans VirtualFS : '{dir_seg}'"
                    )));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tokio::fs::create_dir(&cur).await.map_err(|e| {
                    HakilleError::StorageError(format!("Create dir error {}: {e}", cur.display()))
                })?;
            }
            Err(e) => {
                return Err(HakilleError::StorageError(format!(
                    "Dir metadata error {}: {e}",
                    cur.display()
                )));
            }
        }
        let cur_c = cur.canonicalize().map_err(|e| {
            HakilleError::StorageError(format!("Canonicalize dir error {}: {e}", cur.display()))
        })?;
        if !cur_c.starts_with(&root_canonical) {
            return Err(HakilleError::CapabilityDenied(
                "resolved parent escapes VirtualFS root".into(),
            ));
        }
    }

    let file_path = cur.join(&segments[segments.len() - 1]);
    if let Ok(meta) = tokio::fs::symlink_metadata(&file_path).await
        && meta.file_type().is_symlink()
    {
        return Err(HakilleError::CapabilityDenied(
            "écriture vers un lien symbolique refusée dans le VirtualFS".into(),
        ));
    }

    tokio::fs::write(&file_path, data).await.map_err(|e| {
        HakilleError::StorageError(format!("Write error {}: {e}", file_path.display()))
    })
}

/// Liste confinée d'un répertoire du VirtualFS (sans suivre les liens symboliques).
pub async fn list_confined_dir(root: &Path, path: &VirtualPath) -> Result<Vec<VirtualPath>> {
    let local_path = resolve_confined_path(root, path, false)?;
    let meta = match tokio::fs::symlink_metadata(&local_path).await {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(HakilleError::StorageError(format!(
                "Directory not found: {}",
                path.0
            )));
        }
        Err(e) => {
            return Err(HakilleError::StorageError(format!(
                "List dir metadata error {}: {e}",
                local_path.display()
            )));
        }
    };
    if meta.file_type().is_symlink() {
        return Err(HakilleError::CapabilityDenied(
            "lien symbolique refusé lors du listage VirtualFS".into(),
        ));
    }
    if !meta.is_dir() {
        return Err(HakilleError::StorageError(format!(
            "Directory not found: {}",
            path.0
        )));
    }
    let mut entries = Vec::new();
    let mut read_dir = tokio::fs::read_dir(&local_path).await.map_err(|e| {
        HakilleError::StorageError(format!("Read dir error {}: {e}", local_path.display()))
    })?;
    while let Some(entry) = read_dir.next_entry().await.map_err(|e| {
        HakilleError::StorageError(format!(
            "Read dir entry error {}: {e}",
            local_path.display()
        ))
    })? {
        if let Some(name) = entry.file_name().to_str() {
            entries.push(VirtualPath(name.to_string()));
        }
    }
    Ok(entries)
}

/// Suppression confinée d'un fichier du VirtualFS (refuse liens symboliques et sortie de racine).
pub async fn delete_confined_file(root: &Path, path: &VirtualPath) -> Result<()> {
    let local_path = resolve_confined_path(root, path, false)?;
    match tokio::fs::symlink_metadata(&local_path).await {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(HakilleError::CapabilityDenied(
                    "suppression d'un lien symbolique refusée dans le VirtualFS".into(),
                ));
            }
            tokio::fs::remove_file(&local_path).await.map_err(|e| {
                HakilleError::StorageError(format!("Delete error {}: {e}", local_path.display()))
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(HakilleError::StorageError(format!(
            "Delete metadata error {}: {e}",
            local_path.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_reserved_names_and_nul() {
        for bad in [
            "../etc/passwd",
            r"..\evil.txt",
            "a/../../b",
            "/etc/passwd",
            "C:\\Windows\\win.ini",
            "foo\0bar",
            "CON",
            "dir/nul.txt",
            "COM1.log",
            "lpt9 ",
        ] {
            assert!(
                validate_virtual_components(&VirtualPath(bad.into())).is_err(),
                "le chemin '{bad}' devait être refusé"
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn blocks_symlink_escape_for_read_and_write() {
        let base = std::env::temp_dir().join(format!("hakille-vfs-test-{}", uuid::Uuid::new_v4()));
        let vfs_root = base.join("vfs");
        let outside = base.join("outside");
        std::fs::create_dir_all(&vfs_root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), b"top-secret").unwrap();

        std::os::unix::fs::symlink(&outside, vfs_root.join("escape")).unwrap();

        let r = read_confined_file(&vfs_root, &VirtualPath("escape/secret.txt".into())).await;
        assert!(matches!(r, Err(HakilleError::CapabilityDenied(_))));

        let w =
            write_confined_file(&vfs_root, &VirtualPath("escape/pwned.txt".into()), b"evil").await;
        assert!(matches!(w, Err(HakilleError::CapabilityDenied(_))));
        assert!(!outside.join("pwned.txt").exists());

        let _ = std::fs::remove_dir_all(&base);
    }
}
