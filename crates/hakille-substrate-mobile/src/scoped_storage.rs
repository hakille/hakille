//! Stockage relatif au conteneur fourni par l'hôte. La validation lexicale ne
//! constitue pas une autorisation d'E/S : utiliser AppContainerStorage, dont
//! chaque opération est relative à un handle de répertoire épinglé.
use cap_primitives::fs as capfs;
use hakille_core::{HakilleError, Result, VirtualPath};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAX_FILE_BYTES: usize = 16 * 1024 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StorageScope {
    Documents,
    Cache,
    ScopedExternalMedia,
    SecureKeystore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopedStorageSandbox {
    app_container_root: PathBuf,
    // Les grants ne sont pas des données de configuration restaurables depuis
    // un JSON : seul le fournisseur natif peut les réintroduire explicitement.
    #[serde(skip)]
    permitted_media_uris: HashSet<String>,
}

fn denied(message: impl Into<String>) -> HakilleError {
    HakilleError::StorageError(message.into())
}
fn io_error(context: &str, error: impl std::fmt::Display) -> HakilleError {
    denied(format!("{context} : {error}"))
}

#[cfg(unix)]
fn sync_directory(directory: &File) -> std::io::Result<()> {
    // open_dir peut fournir O_PATH : fsync y échouerait EBADF. Réouvrir « . »
    // depuis le handle, en lecture et sans dir_required, conserve le confinement.
    let mut options = capfs::OpenOptions::new();
    options.read(true)._cap_fs_ext_maybe_dir(true);
    capfs::open(directory, Path::new("."), &options)?.sync_all()
}

/// Syntaxe portable : ni traversée, ADS Windows, séparateur ambigu ni device
/// Windows. « rapport..brouillon.txt » reste un nom ordinaire autorisé.
fn relative_path(path: &VirtualPath, allow_root: bool) -> Result<PathBuf> {
    let text = &path.0;
    if allow_root && (text.is_empty() || text == ".") {
        return Ok(PathBuf::from("."));
    }
    if text.is_empty()
        || text.len() > 4096
        || text.contains(['\\', ':'])
        || text.chars().any(char::is_control)
    {
        return Err(denied("chemin relatif portable attendu"));
    }
    let mut result = PathBuf::new();
    for component in text.split('/') {
        if component.is_empty()
            || matches!(component, "." | "..")
            || component.ends_with([' ', '.'])
        {
            return Err(denied("composant de chemin ambigu ou traversée interdite"));
        }
        let stem = component
            .split('.')
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        let device_suffix = stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"));
        if matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$"
        ) || device_suffix.is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        }) {
            return Err(denied("nom de périphérique Windows interdit"));
        }
        result.push(component);
    }
    Ok(result)
}

fn scope_directory(scope: StorageScope) -> Result<&'static str> {
    match scope {
        StorageScope::Documents => Ok("Documents"),
        StorageScope::Cache => Ok("Cache"),
        StorageScope::ScopedExternalMedia => Err(denied(
            "SAF/PhotoPicker exige une URI et un fournisseur natif ; aucune conversion en chemin local",
        )),
        StorageScope::SecureKeystore => Err(denied(
            "le trousseau sécurisé n'est pas un répertoire ; utiliser hakille-keystore et son fournisseur natif",
        )),
    }
}

impl ScopedStorageSandbox {
    pub fn new(app_container_root: impl Into<PathBuf>) -> Self {
        Self {
            app_container_root: app_container_root.into(),
            permitted_media_uris: HashSet::new(),
        }
    }

    /// Enregistre une autorisation fournie par le picker natif. Cet enregistrement
    /// ne prouve ni l'existence du document ni la validité du grant côté OS.
    pub fn grant_media_access(&mut self, uri: impl Into<String>) -> Result<()> {
        let uri = uri.into();
        let valid = uri
            .strip_prefix("content://")
            .and_then(|rest| rest.split_once('/'))
            .is_some_and(|(authority, path)| {
                !authority.is_empty()
                    && !path.is_empty()
                    && !authority.contains(['@', ':', '?', '#', '\\'])
            });
        if !valid
            || uri.len() > 4096
            || uri.chars().any(|c| c.is_control() || c.is_whitespace())
            || uri.contains('\\')
        {
            return Err(denied("URI SAF content:// explicite attendue"));
        }
        if self.permitted_media_uris.len() >= 256 && !self.permitted_media_uris.contains(&uri) {
            return Err(denied("limite de 256 autorisations SAF atteinte"));
        }
        self.permitted_media_uris.insert(uri);
        Ok(())
    }

    pub fn revoke_media_access(&mut self, uri: &str) -> bool {
        self.permitted_media_uris.remove(uri)
    }
    pub fn has_media_access(&self, uri: &str) -> bool {
        self.permitted_media_uris.contains(uri)
    }

    pub fn read_external_media(&self, uri: &str) -> Result<Vec<u8>> {
        if !self.has_media_access(uri) {
            return Err(denied(
                "accès SAF refusé : aucun grant pour cette URI exacte",
            ));
        }
        Err(denied(
            "lecture SAF indisponible : fournisseur ContentResolver/PhotoPicker natif non raccordé",
        ))
    }

    /// Résolution LEXICALE pour affichage/compatibilité seulement. Ne pas ouvrir
    /// le PathBuf résultant : il ne protège pas contre les symlinks/renommages.
    pub fn resolve_path(&self, scope: StorageScope, path: &VirtualPath) -> Result<PathBuf> {
        Ok(self
            .app_container_root
            .join(scope_directory(scope)?)
            .join(relative_path(path, false)?))
    }

    pub fn container_root(&self) -> &Path {
        &self.app_container_root
    }
}

impl Default for ScopedStorageSandbox {
    fn default() -> Self {
        Self::new("/data/user/0/com.hakille.ai/files")
    }
}

/// E/S réelles à autorité de répertoire. Le conteneur EXISTANT doit provenir du
/// fournisseur hôte de confiance. Les sous-répertoires sont ouverts une fois ;
/// remplacer ultérieurement leur chemin par un symlink ne change pas l'autorité.
pub struct AppContainerStorage {
    sandbox: ScopedStorageSandbox,
    documents: File,
    cache: File,
}

impl AppContainerStorage {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let root_handle = capfs::open_ambient_dir(&root, cap_primitives::ambient_authority())
            .map_err(|e| io_error("ouverture du conteneur hôte", e))?;
        let mut directories = Vec::new();
        for name in ["Documents", "Cache"] {
            let mut options = capfs::DirOptions::new();
            #[cfg(unix)]
            {
                use capfs::DirBuilderExt;
                options.mode(0o700);
            }
            if let Err(error) = capfs::create_dir(&root_handle, Path::new(name), &options)
                && error.kind() != std::io::ErrorKind::AlreadyExists
            {
                return Err(io_error("création du scope", error));
            }
            #[cfg(unix)]
            sync_directory(&root_handle)
                .map_err(|e| io_error("synchronisation des scopes créés", e))?;
            directories.push(
                capfs::open_dir_nofollow(&root_handle, Path::new(name))
                    .map_err(|e| io_error("ouverture du scope sans symlink", e))?,
            );
        }
        let cache = directories.pop().expect("scope Cache créé");
        let documents = directories.pop().expect("scope Documents créé");
        Ok(Self {
            sandbox: ScopedStorageSandbox::new(root),
            documents,
            cache,
        })
    }

    pub fn sandbox(&self) -> &ScopedStorageSandbox {
        &self.sandbox
    }

    fn directory(&self, scope: StorageScope) -> Result<&File> {
        match scope {
            StorageScope::Documents => Ok(&self.documents),
            StorageScope::Cache => Ok(&self.cache),
            _ => {
                scope_directory(scope)?;
                unreachable!()
            }
        }
    }

    pub fn read_file(&self, scope: StorageScope, path: &VirtualPath) -> Result<Vec<u8>> {
        let path = relative_path(path, false)?;
        let mut options = capfs::OpenOptions::new();
        options.read(true);
        // Extension de cap-primitives épinglé : ne pas attendre un producteur
        // si le nom désigne une FIFO, avant le contrôle du type sur le handle.
        #[cfg(unix)]
        options._cap_fs_ext_nonblock(true);
        let file = capfs::open(self.directory(scope)?, &path, &options)
            .map_err(|e| io_error("lecture confinée", e))?;
        let metadata = file
            .metadata()
            .map_err(|e| io_error("métadonnées du fichier", e))?;
        if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES as u64 {
            return Err(denied("fichier ordinaire de 16 Mio maximum attendu"));
        }
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| io_error("lecture du fichier", e))?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(denied("le fichier dépasse 16 Mio pendant la lecture"));
        }
        Ok(bytes)
    }

    /// Fichier temporaire dans le même répertoire, sync puis rename atomique.
    /// Après un échec du sync du répertoire, le message indique explicitement
    /// que le renommage a déjà eu lieu ; aucun rollback n'est prétendu.
    pub fn write_file_atomic(
        &self,
        scope: StorageScope,
        path: &VirtualPath,
        bytes: &[u8],
    ) -> Result<()> {
        if bytes.len() > MAX_FILE_BYTES {
            return Err(denied("écriture limitée à 16 Mio"));
        }
        let path = relative_path(path, false)?;
        let parent = self.open_parent(scope, &path, true)?;
        let name = path
            .file_name()
            .ok_or_else(|| denied("nom de fichier absent"))?;
        let temporary = PathBuf::from(format!(".hakille-write-{}", uuid::Uuid::new_v4()));
        let mut options = capfs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use capfs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = capfs::open(&parent, &temporary, &options)
            .map_err(|e| io_error("création temporaire confinée", e))?;
        let result = (|| {
            file.write_all(bytes)
                .map_err(|e| io_error("écriture temporaire", e))?;
            file.sync_all()
                .map_err(|e| io_error("synchronisation temporaire", e))?;
            capfs::rename(&parent, &temporary, &parent, Path::new(name))
                .map_err(|e| io_error("publication atomique", e))?;
            #[cfg(unix)]
            sync_directory(&parent).map_err(|e| {
                io_error(
                    "fichier déjà remplacé, durabilité du répertoire non confirmée",
                    e,
                )
            })?;
            Ok(())
        })();
        if result.is_err() {
            let _ = capfs::remove_file(&parent, &temporary);
        }
        result
    }

    /// Liste directe, sans récursion ni suivi des entrées symlink. Le handle
    /// du répertoire cible est résolu sous le scope, même en cas de renommage.
    pub fn list_dir(&self, scope: StorageScope, path: &VirtualPath) -> Result<Vec<VirtualPath>> {
        let display_prefix = path.0.clone();
        let path = relative_path(path, true)?;
        let directory = capfs::open_dir(self.directory(scope)?, &path)
            .map_err(|e| io_error("ouverture du répertoire", e))?;
        let entries = capfs::read_base_dir(&directory)
            .map_err(|e| io_error("inventaire du répertoire", e))?;
        let mut result = Vec::new();
        for entry in entries {
            if result.len() >= MAX_DIRECTORY_ENTRIES {
                return Err(denied("répertoire limité à 10000 entrées"));
            }
            let entry = entry.map_err(|e| io_error("entrée de répertoire", e))?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| denied("nom non UTF-8 non représentable"))?;
            relative_path(&VirtualPath(name.clone()), false)?;
            let name = if path == Path::new(".") {
                name
            } else {
                format!("{display_prefix}/{name}")
            };
            result.push(VirtualPath(name));
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }

    pub fn delete_file(&self, scope: StorageScope, path: &VirtualPath) -> Result<()> {
        let path = relative_path(path, false)?;
        let parent = self.open_parent(scope, &path, false)?;
        let name = path
            .file_name()
            .ok_or_else(|| denied("nom de fichier absent"))?;
        capfs::remove_file(&parent, Path::new(name))
            .map_err(|e| io_error("suppression confinée", e))?;
        #[cfg(unix)]
        sync_directory(&parent).map_err(|e| {
            io_error(
                "suppression déjà effectuée, durabilité du répertoire non confirmée",
                e,
            )
        })?;
        Ok(())
    }

    fn open_parent(&self, scope: StorageScope, path: &Path, create: bool) -> Result<File> {
        let mut parent = self
            .directory(scope)?
            .try_clone()
            .map_err(|e| io_error("duplication du scope", e))?;
        if let Some(components) = path.parent() {
            for component in components.components() {
                let part = Path::new(component.as_os_str());
                if create {
                    let mut options = capfs::DirOptions::new();
                    #[cfg(unix)]
                    {
                        use capfs::DirBuilderExt;
                        options.mode(0o700);
                    }
                    if let Err(error) = capfs::create_dir(&parent, part, &options)
                        && error.kind() != std::io::ErrorKind::AlreadyExists
                    {
                        return Err(io_error("création du répertoire parent", error));
                    }
                }
                #[cfg(unix)]
                if create {
                    sync_directory(&parent).map_err(|e| {
                        io_error("répertoire créé, durabilité parent non confirmée", e)
                    })?;
                }
                parent = capfs::open_dir_nofollow(&parent, part)
                    .map_err(|e| io_error("répertoire parent sans symlink", e))?;
            }
        }
        Ok(parent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("hakille-mobile-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn portable_validation_rejects_traversal_ads_and_preserves_dotted_names() {
        let sandbox = ScopedStorageSandbox::new("/container");
        for path in [
            "",
            "..",
            "a/../b",
            "a/./b",
            "/tmp/file",
            "a\\..\\b",
            "C:relative",
            "file:stream",
            "//server/share",
            "a//b",
            "a\0b",
            "CON.txt",
            "COM¹.txt",
            "CONOUT$",
            "x/NUL",
            "trailing.",
            "trailing ",
        ] {
            assert!(
                sandbox
                    .resolve_path(StorageScope::Documents, &VirtualPath(path.into()))
                    .is_err(),
                "{path:?}"
            );
        }
        assert!(
            sandbox
                .resolve_path(
                    StorageScope::Documents,
                    &VirtualPath("rapport..brouillon.txt".into())
                )
                .is_ok()
        );
        assert!(
            sandbox
                .resolve_path(StorageScope::SecureKeystore, &VirtualPath("secret".into()))
                .is_err()
        );
    }

    #[test]
    fn saf_grants_are_exact_revocable_and_not_filesystem_paths() {
        let mut sandbox = ScopedStorageSandbox::new("/container");
        let uri = "content://media/document/photos%3A17";
        sandbox.grant_media_access(uri).unwrap();
        assert!(sandbox.has_media_access(uri));
        assert!(!sandbox.has_media_access("content://media/document/photos:17"));
        assert!(!sandbox.has_media_access(&format!("{uri}/child")));
        assert!(
            sandbox
                .resolve_path(StorageScope::ScopedExternalMedia, &VirtualPath(uri.into()))
                .is_err()
        );
        assert!(
            sandbox
                .read_external_media(uri)
                .unwrap_err()
                .to_string()
                .contains("non raccordé")
        );
        assert!(sandbox.revoke_media_access(uri));
        assert!(!sandbox.revoke_media_access(uri));
        assert!(sandbox.read_external_media(uri).is_err());
        assert!(sandbox.grant_media_access("photos/receipt.jpg").is_err());
    }

    #[test]
    fn actual_storage_roundtrip_atomic_replacement_and_isolation() {
        let directory = TestDirectory::new();
        let storage = AppContainerStorage::open(directory.0.clone()).unwrap();
        let path = VirtualPath("notes/rapport..txt".into());
        storage
            .write_file_atomic(StorageScope::Documents, &path, b"one")
            .unwrap();
        storage
            .write_file_atomic(StorageScope::Documents, &path, b"two")
            .unwrap();
        assert_eq!(
            storage.read_file(StorageScope::Documents, &path).unwrap(),
            b"two"
        );
        assert!(storage.read_file(StorageScope::Cache, &path).is_err());
        assert_eq!(
            storage
                .list_dir(StorageScope::Documents, &VirtualPath("notes".into()))
                .unwrap()[0]
                .0,
            path.0
        );
        assert_eq!(
            storage
                .list_dir(StorageScope::Documents, &VirtualPath(".".into()))
                .unwrap()[0]
                .0,
            "notes"
        );
        storage.delete_file(StorageScope::Documents, &path).unwrap();
        assert!(storage.read_file(StorageScope::Documents, &path).is_err());
        assert!(
            storage
                .write_file_atomic(StorageScope::SecureKeystore, &path, b"secret")
                .is_err()
        );
    }

    #[test]
    fn nested_listing_roundtrips_portable_virtual_paths() {
        let directory = TestDirectory::new();
        let storage = AppContainerStorage::open(directory.0.clone()).unwrap();
        let expected = VirtualPath("first/second/file.txt".into());
        storage
            .write_file_atomic(StorageScope::Documents, &expected, b"nested")
            .unwrap();
        let listed = storage
            .list_dir(StorageScope::Documents, &VirtualPath("first/second".into()))
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0, expected.0);
        assert_eq!(
            storage
                .read_file(StorageScope::Documents, &listed[0])
                .unwrap(),
            b"nested"
        );
        storage
            .delete_file(StorageScope::Documents, &listed[0])
            .unwrap();
        assert!(
            storage
                .read_file(StorageScope::Documents, &expected)
                .is_err()
        );
    }

    #[test]
    fn failed_atomic_replacement_keeps_existing_object_and_cleans_temp() {
        let directory = TestDirectory::new();
        let storage = AppContainerStorage::open(directory.0.clone()).unwrap();
        std::fs::create_dir(directory.0.join("Documents/existing")).unwrap();
        assert!(
            storage
                .write_file_atomic(
                    StorageScope::Documents,
                    &VirtualPath("existing".into()),
                    b"payload"
                )
                .is_err()
        );
        assert!(directory.0.join("Documents/existing").is_dir());
        let entries = storage
            .list_dir(StorageScope::Documents, &VirtualPath("".into()))
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "existing");
        let oversized = std::fs::File::create(directory.0.join("Documents/oversized")).unwrap();
        oversized.set_len(MAX_FILE_BYTES as u64 + 1).unwrap();
        assert!(
            storage
                .read_file(StorageScope::Documents, &VirtualPath("oversized".into()))
                .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn racing_symlink_replacement_never_reads_outside_container() {
        use std::os::unix::fs::symlink;
        let directory = TestDirectory::new();
        let outside = TestDirectory::new();
        let storage = AppContainerStorage::open(directory.0.clone()).unwrap();
        storage
            .write_file_atomic(
                StorageScope::Documents,
                &VirtualPath("inside/payload".into()),
                b"inside",
            )
            .unwrap();
        std::fs::write(outside.0.join("payload"), b"outside").unwrap();
        let link = directory.0.join("Documents/link");
        let outside_path = outside.0.clone();
        let attacker = std::thread::spawn(move || {
            for _ in 0..200 {
                let _ = std::fs::remove_file(&link);
                let _ = symlink("inside", &link);
                let _ = std::fs::remove_file(&link);
                let _ = symlink(&outside_path, &link);
            }
        });
        for _ in 0..400 {
            if let Ok(bytes) =
                storage.read_file(StorageScope::Documents, &VirtualPath("link/payload".into()))
            {
                assert_eq!(bytes, b"inside");
            }
        }
        attacker.join().unwrap();
        assert_eq!(
            std::fs::read(outside.0.join("payload")).unwrap(),
            b"outside"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_refused_and_pinned_scope_survives_replacement() {
        use std::os::unix::fs::symlink;
        let directory = TestDirectory::new();
        let outside = TestDirectory::new();
        let storage = AppContainerStorage::open(directory.0.clone()).unwrap();
        std::fs::write(outside.0.join("secret"), b"outside").unwrap();
        symlink(&outside.0, directory.0.join("Documents/escape")).unwrap();
        let escape = VirtualPath("escape/secret".into());
        assert!(storage.read_file(StorageScope::Documents, &escape).is_err());
        assert!(
            storage
                .write_file_atomic(StorageScope::Documents, &escape, b"changed")
                .is_err()
        );
        assert!(
            storage
                .delete_file(StorageScope::Documents, &escape)
                .is_err()
        );
        assert!(
            storage
                .list_dir(StorageScope::Documents, &VirtualPath("escape".into()))
                .is_err()
        );
        std::fs::rename(
            directory.0.join("Documents"),
            directory.0.join("OriginalDocuments"),
        )
        .unwrap();
        symlink(&outside.0, directory.0.join("Documents")).unwrap();
        storage
            .write_file_atomic(
                StorageScope::Documents,
                &VirtualPath("secret".into()),
                b"inside",
            )
            .unwrap();
        assert_eq!(std::fs::read(outside.0.join("secret")).unwrap(), b"outside");
        assert_eq!(
            std::fs::read(directory.0.join("OriginalDocuments/secret")).unwrap(),
            b"inside"
        );
        assert!(AppContainerStorage::open(directory.0.clone()).is_err());
    }
}
