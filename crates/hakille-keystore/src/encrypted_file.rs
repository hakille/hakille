//! Repli hors production : Argon2id + ChaCha20Poly1305, format authentifié v2.
//! Le fichier de verrou reste en place : le supprimer créerait deux domaines
//! de verrouillage concurrents. Les appels coopérants utilisent tous ce verrou.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::{KeystoreError, LoadOutcome, SEED_LEN, Seed};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    ChaCha20Poly1305, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use zeroize::Zeroizing;

pub const ENCRYPTED_FILE_MAGIC: &[u8; 8] = b"HAKKEY02";
pub const ENCRYPTED_FILE_VERSION: u32 = 2;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const HEADER_LEN: usize = 8 + 4 + SALT_LEN + NONCE_LEN;
const FILE_LEN: usize = HEADER_LEN + SEED_LEN + 16;

fn io_error(error: std::io::Error) -> KeystoreError {
    KeystoreError::FileFallback(format!("E/S du repli : {error}"))
}

fn authenticate_error() -> KeystoreError {
    KeystoreError::FileFallback(
        "authentification refusée : mot de passe incorrect ou fichier altéré".into(),
    )
}

fn check_policy(password: &[u8], is_prod: bool) -> Result<(), KeystoreError> {
    if is_prod {
        return Err(KeystoreError::InsecureBoot(
            "prod refuse le repli fichier : déverrouillez le trousseau OS".into(),
        ));
    }
    if password.is_empty() {
        return Err(KeystoreError::InsecureBoot(
            "le repli exige un mot de passe non vide".into(),
        ));
    }
    Ok(())
}

fn is_prod() -> bool {
    std::env::var("HAKILLE_PROD")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn derive_key(password: &[u8], salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, KeystoreError> {
    // Paramètres fixes du format v2 : 19 Mio, deux passes, une voie.
    // Aucun paramètre non authentifié fourni par le fichier ne pilote l'allocation.
    let params = Params::new(19 * 1024, 2, 1, Some(32))
        .map_err(|_| KeystoreError::FileFallback("paramètres KDF invalides".into()))?;
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password, salt, &mut key[..])
        .map_err(|_| KeystoreError::FileFallback("dérivation de clé impossible".into()))?;
    Ok(key)
}

fn fresh_seed() -> Result<Seed, KeystoreError> {
    let mut seed = Zeroizing::new([0u8; SEED_LEN]);
    getrandom::getrandom(&mut seed[..]).map_err(|e| KeystoreError::Randomness(e.to_string()))?;
    Ok(seed)
}

fn seal(seed: &Seed, password: &[u8]) -> Result<Vec<u8>, KeystoreError> {
    let mut header = [0u8; HEADER_LEN];
    header[..8].copy_from_slice(ENCRYPTED_FILE_MAGIC);
    header[8..12].copy_from_slice(&ENCRYPTED_FILE_VERSION.to_be_bytes());
    getrandom::getrandom(&mut header[12..])
        .map_err(|e| KeystoreError::Randomness(e.to_string()))?;
    let key = derive_key(password, &header[12..12 + SALT_LEN])?;
    let ciphertext = ChaCha20Poly1305::new_from_slice(&key[..])
        .map_err(|_| KeystoreError::FileFallback("clé de chiffrement invalide".into()))?
        .encrypt(
            Nonce::from_slice(&header[12 + SALT_LEN..]),
            Payload {
                msg: &seed[..],
                aad: &header,
            },
        )
        .map_err(|_| KeystoreError::FileFallback("chiffrement impossible".into()))?;
    let mut raw = header.to_vec();
    raw.extend_from_slice(&ciphertext);
    Ok(raw)
}

fn unseal(raw: &[u8], password: &[u8]) -> Result<Seed, KeystoreError> {
    if raw.starts_with(b"ANRKEY01") {
        return Err(KeystoreError::FileFallback("ancien format v1 non authentifié refusé ; fichier conservé, migration explicite depuis une sauvegarde de confiance requise".into()));
    }
    if raw.len() != FILE_LEN
        || &raw[..8] != ENCRYPTED_FILE_MAGIC
        || raw[8..12] != ENCRYPTED_FILE_VERSION.to_be_bytes()
    {
        return Err(KeystoreError::FileFallback(
            "format de repli invalide ou version non prise en charge".into(),
        ));
    }
    let key = derive_key(password, &raw[12..12 + SALT_LEN])?;
    let plaintext = Zeroizing::new(
        ChaCha20Poly1305::new_from_slice(&key[..])
            .map_err(|_| authenticate_error())?
            .decrypt(
                Nonce::from_slice(&raw[12 + SALT_LEN..HEADER_LEN]),
                Payload {
                    msg: &raw[HEADER_LEN..],
                    aad: &raw[..HEADER_LEN],
                },
            )
            .map_err(|_| authenticate_error())?,
    );
    let mut seed = Zeroizing::new([0u8; SEED_LEN]);
    seed.copy_from_slice(&plaintext);
    Ok(seed)
}

fn reject_symlink(path: &Path) -> Result<(), KeystoreError> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(KeystoreError::FileFallback(
            "lien symbolique interdit pour le repli ou son verrou".into(),
        )),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_error(e)),
    }
}

fn options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    options
}

fn read_existing(path: &Path) -> Result<Option<Vec<u8>>, KeystoreError> {
    reject_symlink(path)?;
    let file = match options().open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error(e)),
    };
    if !file.metadata().map_err(io_error)?.is_file() {
        return Err(KeystoreError::FileFallback(
            "repli non régulier refusé".into(),
        ));
    }
    let mut raw = Vec::new();
    file.take((FILE_LEN + 1) as u64)
        .read_to_end(&mut raw)
        .map_err(io_error)?;
    Ok(Some(raw))
}

fn locked_path(path: &Path, create_parent: bool) -> Result<(PathBuf, File), KeystoreError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if create_parent {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)
                .map_err(io_error)?;
        }
        #[cfg(not(unix))]
        std::fs::create_dir_all(parent).map_err(io_error)?;
    }
    let parent = parent.canonicalize().map_err(io_error)?;
    let name = path
        .file_name()
        .ok_or_else(|| KeystoreError::FileFallback("nom de fichier manquant".into()))?;
    let path = parent.join(name);
    let mut lock_name = name.to_os_string();
    lock_name.push(".lock");
    let lock_path = parent.join(lock_name);
    reject_symlink(&lock_path)?;
    let lock = options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
        .map_err(io_error)?;
    if !lock.metadata().map_err(io_error)?.is_file() {
        return Err(KeystoreError::FileFallback(
            "verrou non régulier refusé".into(),
        ));
    }
    lock.lock().map_err(io_error)?;
    Ok((path, lock))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), KeystoreError> {
    let parent = path
        .parent()
        .ok_or_else(|| KeystoreError::FileFallback("répertoire absent".into()))?;
    // NamedTempFile crée en 0600 dès l'ouverture sur Unix ; aucune fenêtre
    // avec un seed dans un fichier temporaire lisible par les autres comptes.
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(io_error)?;
    temporary.write_all(bytes).map_err(io_error)?;
    temporary.as_file().sync_all().map_err(io_error)?;
    temporary.persist(path).map_err(|e| io_error(e.error))?;
    #[cfg(unix)]
    File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(io_error)?;
    Ok(())
}

/// Charge ou crée un seed authentifié. Un échec ne régénère jamais un seed existant.
pub fn load_or_create_encrypted_file(
    path: &Path,
    password: &[u8],
) -> Result<LoadOutcome, KeystoreError> {
    load_or_create_encrypted_file_impl(path, password, is_prod())
}

pub(crate) fn load_or_create_encrypted_file_impl(
    path: &Path,
    password: &[u8],
    is_prod: bool,
) -> Result<LoadOutcome, KeystoreError> {
    check_policy(password, is_prod)?;
    let (path, _lock) = locked_path(path, true)?;
    if let Some(raw) = read_existing(&path)? {
        return Ok(LoadOutcome {
            seed: unseal(&raw, password)?,
            created: false,
        });
    }
    let seed = fresh_seed()?;
    atomic_write(&path, &seal(&seed, password)?)?;
    verify_written(&path, password, &seed)?;
    Ok(LoadOutcome {
        seed,
        created: true,
    })
}

fn verify_written(path: &Path, password: &[u8], expected: &Seed) -> Result<(), KeystoreError> {
    let raw = read_existing(path)?
        .ok_or_else(|| KeystoreError::FileFallback("fichier disparu après écriture".into()))?;
    if *unseal(&raw, password)? != **expected {
        return Err(KeystoreError::VerificationFailed {
            user: path.display().to_string(),
            reason: "relecture différente après écriture".into(),
        });
    }
    Ok(())
}

/// Change le seed et le mot de passe après authentification de l'ancien fichier.
/// La rotation invalide les usages de l'ancienne clé ; elle n'est pas un simple
/// changement de mot de passe. Le verrou sérialise toute l'opération.
pub fn rotate_encrypted_file(
    path: &Path,
    old_password: &[u8],
    new_password: &[u8],
) -> Result<(), KeystoreError> {
    rotate_encrypted_file_impl(path, old_password, new_password, is_prod())
}

pub(crate) fn rotate_encrypted_file_impl(
    path: &Path,
    old_password: &[u8],
    new_password: &[u8],
    is_prod: bool,
) -> Result<(), KeystoreError> {
    check_policy(old_password, is_prod)?;
    check_policy(new_password, is_prod)?;
    let (path, _lock) = locked_path(path, false)?;
    let raw = read_existing(&path)?.ok_or_else(|| KeystoreError::MissingBaseline {
        user: path.display().to_string(),
    })?;
    let _baseline = unseal(&raw, old_password)?;
    let seed = fresh_seed()?;
    atomic_write(&path, &seal(&seed, new_password)?)?;
    verify_written(&path, new_password, &seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrong_password_rotation_preserves_original_file_and_seed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key.enc");
        let initial = load_or_create_encrypted_file_impl(&path, b"correct", false).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(rotate_encrypted_file_impl(&path, b"wrong", b"new", false).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(
            *load_or_create_encrypted_file_impl(&path, b"correct", false)
                .unwrap()
                .seed,
            *initial.seed
        );
    }

    #[test]
    fn corruption_of_header_salt_nonce_ciphertext_and_tag_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key.enc");
        load_or_create_encrypted_file_impl(&path, b"password", false).unwrap();
        let original = std::fs::read(&path).unwrap();
        for offset in [0, 8, 12, 28, HEADER_LEN, FILE_LEN - 1] {
            let mut corrupt = original.clone();
            corrupt[offset] ^= 1;
            std::fs::write(&path, &corrupt).unwrap();
            assert!(
                load_or_create_encrypted_file_impl(&path, b"password", false).is_err(),
                "altération à {offset}"
            );
            assert_eq!(std::fs::read(&path).unwrap(), corrupt);
        }
        for raw in [&original[..FILE_LEN - 1], &[0u8; FILE_LEN + 1][..]] {
            std::fs::write(&path, raw).unwrap();
            assert!(load_or_create_encrypted_file_impl(&path, b"password", false).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), raw);
        }
    }

    #[test]
    fn legacy_format_is_preserved_and_requires_explicit_migration() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key.enc");
        let mut legacy = vec![0u8; 60];
        legacy[..8].copy_from_slice(b"ANRKEY01");
        legacy[8..12].copy_from_slice(&1u32.to_be_bytes());
        std::fs::write(&path, &legacy).unwrap();
        let error = load_or_create_encrypted_file_impl(&path, b"password", false).unwrap_err();
        assert!(error.to_string().contains("migration explicite"));
        assert!(rotate_encrypted_file_impl(&path, b"password", b"new", false).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), legacy);
    }

    #[test]
    fn simultaneous_creation_returns_one_persisted_seed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key.enc");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let workers: Vec<_> = (0..2)
            .map(|_| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    load_or_create_encrypted_file_impl(&path, b"password", false).unwrap()
                })
            })
            .collect();
        barrier.wait();
        let outcomes: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
        assert_eq!(outcomes.iter().filter(|o| o.created).count(), 1);
        assert_eq!(*outcomes[0].seed, *outcomes[1].seed);
    }

    #[test]
    fn concurrent_rotations_cannot_overwrite_a_new_password_with_old_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key.enc");
        load_or_create_encrypted_file_impl(&path, b"old", false).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let workers: Vec<_> = [b"new-a", b"new-b"]
            .into_iter()
            .map(|password| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    (
                        password,
                        rotate_encrypted_file_impl(&path, b"old", password, false),
                    )
                })
            })
            .collect();
        barrier.wait();
        let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|(_, r)| r.is_ok()).count(), 1);
        let password = results.iter().find(|(_, r)| r.is_ok()).unwrap().0;
        assert!(load_or_create_encrypted_file_impl(&path, password, false).is_ok());
        assert!(load_or_create_encrypted_file_impl(&path, b"old", false).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn permissions_are_private_and_symlinks_are_never_followed() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key.enc");
        load_or_create_encrypted_file_impl(&path, b"password", false).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(dir.path().join("key.enc.lock"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let link = dir.path().join("link.enc");
        symlink(&path, &link).unwrap();
        let original = std::fs::read(&path).unwrap();
        assert!(load_or_create_encrypted_file_impl(&link, b"password", false).is_err());
        let locked = dir.path().join("other.enc");
        symlink(&path, dir.path().join("other.enc.lock")).unwrap();
        assert!(load_or_create_encrypted_file_impl(&locked, b"password", false).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(!locked.exists());
    }

    #[test]
    fn debug_does_not_expose_seeds() {
        let outcome = LoadOutcome {
            seed: Zeroizing::new([99u8; 32]),
            created: false,
        };
        assert!(!format!("{outcome:?}").contains("99"));
        let store = crate::MemoryStore::new();
        crate::SecretStore::set_secret(&store, "service", "user", &[99u8; 32]).unwrap();
        assert!(!format!("{store:?}").contains("99"));
    }
}
