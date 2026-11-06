//! Identité au boot : aucune régénération après corruption, aucun changement
//! implicite de backend lorsqu'une paire de fichiers existe déjà.

use hakille_core::{CapabilityAuthority, KeyPair};
use hakille_keystore::{KeyRegistry, KeystoreError, SecretStore, Seed};
use std::io::{self, Read, Write};
use std::path::Path;

type Identity = (CapabilityAuthority, KeyPair);
type BootResult<T> = Result<T, Box<dyn std::error::Error>>;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn exists(path: &Path) -> io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

fn read_seed(path: &Path) -> io::Result<Seed> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(invalid(
            "clé : fichier régulier requis, lien symbolique refusé",
        ));
    }
    let mut file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(invalid("clé : fichier régulier requis"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let current = std::fs::symlink_metadata(path)?;
        if !current.is_file() || current.dev() != metadata.dev() || current.ino() != metadata.ino()
        {
            return Err(invalid("clé remplacée pendant l'ouverture : boot refusé"));
        }
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "clé lisible par d'autres utilisateurs : permissions 0600 requises",
            ));
        }
    }
    if metadata.len() != 32 {
        return Err(invalid(
            "clé corrompue : 32 octets requis, fichier conservé",
        ));
    }
    let mut seed = Seed::new([0; 32]);
    file.read_exact(&mut seed[..])?;
    let mut extra = [0u8; 1];
    if file.read(&mut extra)? != 0 {
        return Err(invalid(
            "clé modifiée pendant la lecture : fichier conservé",
        ));
    }
    Ok(seed)
}

fn load_or_create_seed(path: &Path, generate: impl FnOnce() -> Seed) -> io::Result<Seed> {
    match read_seed(path) {
        Ok(seed) => return Ok(seed),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return read_seed(path),
        Err(error) => return Err(error),
    };
    let seed = generate();
    file.write_all(&seed[..])?;
    file.sync_all()?;
    // Échec/arrêt après création : jamais supprimer/régénérer le fichier
    // partiel au prochain boot. L'opérateur doit restaurer l'identité.
    Ok(seed)
}

fn derive(root: &Seed, journal: &Seed) -> BootResult<Identity> {
    let private = biscuit_auth::PrivateKey::from_bytes(&root[..])?;
    Ok((
        CapabilityAuthority::with_keypair(biscuit_auth::KeyPair::from(&private)),
        KeyPair::from_bytes(journal),
    ))
}

fn raw_identity(keys_dir: &Path, create: bool) -> BootResult<Identity> {
    let root_path = keys_dir.join("capability_root.bin");
    let journal_path = keys_dir.join("signing_key.bin");
    let (root, journal) = if create {
        let root = load_or_create_seed(&root_path, || {
            let authority = CapabilityAuthority::new();
            let mut seed = Seed::new([0; 32]);
            seed.copy_from_slice(&authority.root_keypair.private().to_bytes());
            seed
        })?;
        let journal =
            load_or_create_seed(&journal_path, || Seed::new(KeyPair::generate().to_bytes()))?;
        (root, journal)
    } else {
        // Une seule moitié absente est une erreur, jamais un premier boot.
        (read_seed(&root_path)?, read_seed(&journal_path)?)
    };
    derive(&root, &journal)
}

/// Empêche KeyRegistry::load_or_create de générer une identité lorsque le
/// répertoire de données contient déjà un historique signé.
struct BootStore<S> {
    inner: S,
    allow_create: bool,
}
impl<S: SecretStore> SecretStore for BootStore<S> {
    fn get_secret(&self, service: &str, user: &str) -> Result<Option<Vec<u8>>, KeystoreError> {
        self.inner.get_secret(service, user)
    }
    fn set_secret(&self, service: &str, user: &str, secret: &[u8]) -> Result<(), KeystoreError> {
        if !self.allow_create {
            return Err(KeystoreError::InsecureBoot(
                "historique existant mais clé absente ; restauration explicite requise".into(),
            ));
        }
        self.inner.set_secret(service, user, secret)
    }
    fn delete(&self, service: &str, user: &str) -> Result<(), KeystoreError> {
        self.inner.delete(service, user)
    }
}

pub fn load_identity<S: SecretStore>(
    store: S,
    keys_dir: &Path,
    history_exists: bool,
    production: bool,
) -> BootResult<Identity> {
    let raw_exists = exists(&keys_dir.join("capability_root.bin"))?
        || exists(&keys_dir.join("signing_key.bin"))?;
    if raw_exists {
        if production {
            return Err(KeystoreError::InsecureBoot("identité fichier brute existante : migration explicite vers le trousseau requise en production".into()).into());
        }
        eprintln!(
            "  Keys: identité fichier existante conservée (brut non chiffré) ; aucune migration implicite vers le trousseau."
        );
        return raw_identity(keys_dir, false);
    }
    let registry = KeyRegistry::new(BootStore {
        inner: store,
        allow_create: !history_exists,
    });
    let root = match registry.capability_root() {
        Ok(root) => root,
        Err(KeystoreError::Keychain { .. }) if !production && !history_exists => {
            eprintln!(
                "  Keys: trousseau inaccessible au premier boot ; repli fichier brut de développement, permissions 0600 sur Unix."
            );
            return raw_identity(keys_dir, true);
        }
        Err(error) => return Err(error.into()),
    };
    // Dès qu'une clé est chargée, une erreur sur la seconde arrête le boot.
    // Une corruption ne déclenche jamais de changement de backend.
    let journal = registry.journal_signing()?;
    println!("  Keys: identité du trousseau OS chargée (aucun repli brut).");
    derive(&root.seed, &journal.seed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_keystore::{MemoryStore, SERVICE};

    fn unavailable_store() -> MemoryStore {
        let store = MemoryStore::new();
        store.inject_error(Some("trousseau verrouillé".into()));
        store
    }

    #[test]
    fn raw_identity_is_stable_and_not_migrated_implicitly() {
        let dir = tempfile::tempdir().unwrap();
        let (first, _) = load_identity(unavailable_store(), dir.path(), false, false).unwrap();
        let (second, _) = load_identity(MemoryStore::new(), dir.path(), true, false).unwrap();
        assert_eq!(
            first.root_public_key().to_bytes(),
            second.root_public_key().to_bytes()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(dir.path().join("signing_key.bin"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn corrupted_seed_is_preserved_and_never_regenerated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("seed");
        load_or_create_seed(&path, || Seed::new([3; 32])).unwrap();
        std::fs::write(&path, b"corrupt").unwrap();
        assert!(load_or_create_seed(&path, || panic!("aucune génération")).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"corrupt");
    }

    #[test]
    fn nonregular_seed_is_refused_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("seed");
        std::fs::create_dir(&path).unwrap();
        assert!(load_or_create_seed(&path, || panic!("aucune génération")).is_err());
        assert!(path.is_dir());
    }

    #[test]
    fn partial_raw_identity_is_not_completed() {
        let dir = tempfile::tempdir().unwrap();
        load_or_create_seed(&dir.path().join("signing_key.bin"), || Seed::new([3; 32])).unwrap();
        assert!(load_identity(MemoryStore::new(), dir.path(), true, false).is_err());
        assert!(!dir.path().join("capability_root.bin").exists());
    }

    #[test]
    fn production_and_existing_history_refuse_raw_fallback() {
        for (production, history) in [(true, false), (false, true)] {
            let dir = tempfile::tempdir().unwrap();
            assert!(load_identity(unavailable_store(), dir.path(), history, production).is_err());
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn corrupt_keychain_never_creates_raw_identity() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::new();
        store
            .set_secret(SERVICE, "capability-root-v1", b"corrupt")
            .unwrap();
        assert!(load_identity(store.clone(), dir.path(), false, false).is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        assert_eq!(
            store
                .get_secret(SERVICE, "capability-root-v1")
                .unwrap()
                .unwrap(),
            b"corrupt"
        );
        assert!(
            store
                .get_secret(SERVICE, "journal-signing-v1")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn existing_history_never_creates_missing_keychain_half() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::new();
        store
            .set_secret(SERVICE, "capability-root-v1", &[3; 32])
            .unwrap();
        assert!(load_identity(store.clone(), dir.path(), true, false).is_err());
        assert!(
            store
                .get_secret(SERVICE, "journal-signing-v1")
                .unwrap()
                .is_none()
        );
    }

    #[cfg(unix)]
    #[test]
    fn permissive_seed_and_symlink_are_refused() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("seed");
        load_or_create_seed(&path, || Seed::new([3; 32])).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            read_seed(&path).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        let link = dir.path().join("link");
        symlink(&path, &link).unwrap();
        assert!(load_or_create_seed(&link, || panic!("aucune génération")).is_err());
        assert_eq!(std::fs::read(path).unwrap(), [3; 32]);
    }
}
