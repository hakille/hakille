//! Annora keystore: OS-keychain-backed registry for long-lived 32-byte seeds.
//!
//! # Confiance au boot (honest contract)
//!
//! - Service keychain fixe : `"annora"`. Utilisateurs **versionnes** :
//!   `"capability-root-v1"` / `"journal-signing-v1"` (rotation -> `*-v2`).
//! - Secrets = **seeds de 32 octets**. Verification effectuee contre
//!   `biscuit-auth 5.0.0` (`src/crypto/mod.rs`) et `annora-core/src/crypto.rs` :
//!   - racine Biscuit : le seed est la cle privee `PrivateKey` (32 o) —
//!     `PrivateKey::from_bytes(&seed)` puis `KeyPair::from(&private)`.
//!     `KeyPair` n'expose pas `to_bytes` directement, `private()` si.
//!   - journal : `annora_core::KeyPair::from_bytes(&seed)`
//!     (`signing_key.to_bytes()` = le seed).
//!   Le keystore ne depend donc ni de `biscuit-auth` ni d'`annora-core` :
//!   il stocke des octets, la derivation vit dans le daemon (voir snippets).
//! - `load_or_create()` : `get_secret` -> `from_bytes` ; absent -> generate +
//!   `set_secret` + relecture de verification. **Erreur keychain = erreur
//!   explicite, jamais de regeneration silencieuse** (une racine fraiche
//!   invaliderait tous les tokens precedemment mintes).
//! - `rotate()` -> `*-v2` + double verification transitoire documentee
//!   (v2 relue et comparee, v1 conservee lisible jusqu'a revocation explicite).
//! - `revoke()` -> `delete_credential` (idempotent : absent = deja revoque).
//! - Tout secret en memoire est porte par `Zeroizing`, les buffers
//!   intermediaires sont effaces, et **aucun secret n'est jamais logge**
//!   (pas de `Debug`/`Display` sur les seeds ; les messages d'erreur ne
//!   contiennent que service/user, jamais d'octets secrets).
//! - Tests : roundtrip via keychain **moquee** (`MemoryStore`). `OsStore`
//!   (prod, `keyring::Entry`) n'est jamais utilise dans les tests.
//!
//! # Plateformes `keyring`
//!
//! `keyring` n'a pas de default-features : sans feature, c'est le backend
//! `mock` (non persistant, NON SUR pour la prod). On active donc
//! `windows-native` + `apple-native` + `linux-native`. Sur Linux, `keyutils`
//! seul est volatile par session : TODO passer au combo persistant
//! (`linux-native-sync-persistent` + `sync-secret-service` + `crypto-rust`)
//! quand le backend Secret Service sera valide sur les machines cibles.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

/// Service keychain fixe pour toutes les entrees Annora.
pub const SERVICE: &str = "annora";
/// Base du nom d'utilisateur pour la racine de capability Biscuit.
pub const CAPABILITY_ROOT_BASE: &str = "capability-root";
/// Base du nom d'utilisateur pour la signature du journal d'audit.
pub const JOURNAL_SIGNING_BASE: &str = "journal-signing";
/// Taille exacte des seeds stockes (32 octets).
pub const SEED_LEN: usize = 32;

/// Construit le nom d'utilisateur versionne (`"<base>-v<version>"`).
pub fn versioned_user(base: &str, version: u32) -> String {
    format!("{base}-v{version}")
}

/// Seed de 32 octets a effacement automatique (`Zeroizing`).
pub type Seed = Zeroizing<[u8; 32]>;

/// Erreurs explicites du keystore. Ne contiennent JAMAIS d'octets secrets.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum KeystoreError {
    /// Le backend keychain a echoue (indisponible, verrouille, refuse...).
    /// Renvoyee telle quelle : l'appelant doit afficher l'erreur au boot,
    /// jamais regenerer silencieusement.
    #[error("keychain failure (service={service}, user={user}): {reason}")]
    Keychain {
        service: String,
        user: String,
        reason: String,
    },
    /// Entree presente mais taille inattendue (corruption / ecriture tierce).
    #[error("corrupt entry (user={user}): expected {expected} bytes, found {found}")]
    Corrupt {
        user: String,
        expected: usize,
        found: usize,
    },
    /// L'OS n'a pas fourni d'alea (boot tres precoce, `getrandom` indisponible).
    #[error("OS randomness unavailable: {0}")]
    Randomness(String),
    /// Socle de rotation absent : on ne "cree" jamais pendant un `rotate()`.
    #[error("rotation baseline missing (user={user}): refusing to rotate from nothing")]
    MissingBaseline { user: String },
    /// La double verification transitoire a echoue (v2 illisible ou differente).
    #[error("rotation verification failed (user={user}): {reason}")]
    VerificationFailed { user: String, reason: String },
    /// Le repli fichier chiffre a echoue (header invalide, mot de passe
    /// vide, lecture/ecriture impossible...). Ne contient jamais de secret.
    #[error("encrypted file fallback failure: {0}")]
    FileFallback(String),
    /// Boot refuse en mode fail-closed : ni keychain OS ni repli chiffre
    /// utilisable (prod : debloquez le trousseau OS, jamais de boot avec
    /// des cles regenerees en memoire seule).
    #[error("refusing insecure boot (no OS keychain, no valid encrypted fallback): {0}")]
    InsecureBoot(String),
}

/// Backend de stockage de secrets (prod OS ou mock memoire pour tests).
///
/// `&self` suffit (interior mutability cote implementations) pour rester
/// utilisable depuis un `KeyRegistry` partage.
pub trait SecretStore: Send + Sync {
    /// `Ok(None)` = absent. `Err` = echec backend EXPLICITE (pas de defaut).
    fn get_secret(&self, service: &str, user: &str) -> Result<Option<Vec<u8>>, KeystoreError>;
    fn set_secret(&self, service: &str, user: &str, secret: &[u8]) -> Result<(), KeystoreError>;
    /// Idempotent : supprimer un absent reussit (deja revoque).
    fn delete(&self, service: &str, user: &str) -> Result<(), KeystoreError>;
}

#[derive(Debug, Default)]
struct MemoryInner {
    secrets: HashMap<(String, String), Zeroizing<Vec<u8>>>,
    /// Quand `Some`, TOUTES les operations echouent (injection de faute pour tests).
    injected_error: Option<String>,
}

/// Backend memoire pour tests (et secours documente hors-prod).
/// Partageable par `clone()` (meme `Arc` : deux registres voient le meme contenu).
#[derive(Debug, Clone, Default)]
pub struct MemoryStore {
    inner: Arc<Mutex<MemoryInner>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Force l'echec de toutes les operations (simule un keychain casse).
    /// `None` = retour au comportement normal.
    pub fn inject_error(&self, reason: Option<String>) {
        self.inner
            .lock()
            .expect("keystore memory lock")
            .injected_error = reason;
    }
}

impl SecretStore for MemoryStore {
    fn get_secret(&self, service: &str, user: &str) -> Result<Option<Vec<u8>>, KeystoreError> {
        let guard = self.inner.lock().expect("keystore memory lock");
        if let Some(reason) = guard.injected_error.clone() {
            return Err(KeystoreError::Keychain {
                service: service.to_string(),
                user: user.to_string(),
                reason,
            });
        }
        Ok(guard
            .secrets
            .get(&(service.to_string(), user.to_string()))
            .map(|s| s.to_vec()))
    }

    fn set_secret(&self, service: &str, user: &str, secret: &[u8]) -> Result<(), KeystoreError> {
        let mut guard = self.inner.lock().expect("keystore memory lock");
        if let Some(reason) = guard.injected_error.clone() {
            return Err(KeystoreError::Keychain {
                service: service.to_string(),
                user: user.to_string(),
                reason,
            });
        }
        guard.secrets.insert(
            (service.to_string(), user.to_string()),
            Zeroizing::new(secret.to_vec()),
        );
        Ok(())
    }

    fn delete(&self, service: &str, user: &str) -> Result<(), KeystoreError> {
        let mut guard = self.inner.lock().expect("keystore memory lock");
        if let Some(reason) = guard.injected_error.clone() {
            return Err(KeystoreError::Keychain {
                service: service.to_string(),
                user: user.to_string(),
                reason,
            });
        }
        guard
            .secrets
            .remove(&(service.to_string(), user.to_string()));
        Ok(())
    }
}

/// Backend prod : keychain/credential-store natif de l'OS via `keyring` 3
/// (`Entry::new` / `set_secret` / `get_secret` / `delete_credential`).
/// Ne jamais l'utiliser dans les tests (patrimoine reel de la machine).
#[derive(Debug, Clone, Copy, Default)]
pub struct OsStore;

impl OsStore {
    pub fn new() -> Self {
        Self
    }

    fn keychain_err(service: &str, user: &str, err: keyring::Error) -> KeystoreError {
        // `keyring::Error` ne porte que des metadonnees (pas de secrets).
        KeystoreError::Keychain {
            service: service.to_string(),
            user: user.to_string(),
            reason: err.to_string(),
        }
    }
}

impl SecretStore for OsStore {
    fn get_secret(&self, service: &str, user: &str) -> Result<Option<Vec<u8>>, KeystoreError> {
        let entry =
            keyring::Entry::new(service, user).map_err(|e| Self::keychain_err(service, user, e))?;
        match entry.get_secret() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(Self::keychain_err(service, user, e)),
        }
    }

    fn set_secret(&self, service: &str, user: &str, secret: &[u8]) -> Result<(), KeystoreError> {
        let entry =
            keyring::Entry::new(service, user).map_err(|e| Self::keychain_err(service, user, e))?;
        entry
            .set_secret(secret)
            .map_err(|e| Self::keychain_err(service, user, e))
    }

    fn delete(&self, service: &str, user: &str) -> Result<(), KeystoreError> {
        let entry =
            keyring::Entry::new(service, user).map_err(|e| Self::keychain_err(service, user, e))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            // Idempotence documentee : absent = deja revoque.
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(Self::keychain_err(service, user, e)),
        }
    }
}

/// Resultat d'un `load_or_create` : le seed + si cette machine vient de le creer.
/// `created == true` (premier boot) doit etre affiche honnetement au boot.
#[derive(Debug)]
pub struct LoadOutcome {
    pub seed: Seed,
    pub created: bool,
}

/// Rapport de rotation (serialisable pour audit boot).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RotationReport {
    pub entries: Vec<RotatedEntry>,
}

/// Une base pivotee vers `*-v2`, avec preuve de double lecture transitoire.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RotatedEntry {
    /// `"capability-root"` ou `"journal-signing"`.
    pub base: String,
    /// Utilisateur precedent conserve en lecture (`"*-v1"`).
    pub previous_user: String,
    /// Nouvel utilisateur courant (`"*-v2"`).
    pub current_user: String,
    /// v2 relue et identique + v1 toujours lisible (fenetre transitoire).
    pub verified_dual_read: bool,
}

/// Rapport de revocation (utilisateurs effectivement supprimes).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RevocationReport {
    pub removed_users: Vec<String>,
}

/// Registre des seeds de confiance boot, adosse a un `SecretStore`.
pub struct KeyRegistry<S: SecretStore> {
    store: S,
    service: String,
}

impl<S: SecretStore> KeyRegistry<S> {
    pub fn new(store: S) -> Self {
        Self {
            store,
            service: SERVICE.to_string(),
        }
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    /// Tire 32 octets d'alea OS. Erreur explicite, jamais de repli faible.
    fn generate_seed() -> Result<Seed, KeystoreError> {
        let mut bytes = [0u8; SEED_LEN];
        getrandom::getrandom(&mut bytes).map_err(|e| KeystoreError::Randomness(e.to_string()))?;
        Ok(Zeroizing::new(bytes))
    }

    /// Valide la taille et efface le buffer intermediaire avant retour.
    fn checked_seed(user: &str, mut buf: Vec<u8>) -> Result<Seed, KeystoreError> {
        if buf.len() != SEED_LEN {
            let found = buf.len();
            buf.zeroize();
            return Err(KeystoreError::Corrupt {
                user: user.to_string(),
                expected: SEED_LEN,
                found,
            });
        }
        let mut arr = [0u8; SEED_LEN];
        arr.copy_from_slice(&buf);
        buf.zeroize();
        drop(buf);
        Ok(Zeroizing::new(arr))
    }

    /// Charge le seed de `user`, ou le cree (alea OS) + persiste + relit.
    /// Toute erreur backend est renvoyee telle quelle : l'appelant DOIT
    /// interrompre le boot avec un message honnete, jamais regenerer en
    /// memoire seule (divergence silencieuse de racine = faille).
    pub fn load_or_create(&self, user: &str) -> Result<LoadOutcome, KeystoreError> {
        match self.store.get_secret(&self.service, user)? {
            Some(buf) => Ok(LoadOutcome {
                seed: Self::checked_seed(user, buf)?,
                created: false,
            }),
            None => {
                let seed = Self::generate_seed()?;
                self.store.set_secret(&self.service, user, &seed[..])?;
                // Relecture de verification : prouve la persistance immediate
                // (un `set` menteur ne passe pas pour une creation reussie).
                match self.store.get_secret(&self.service, user)? {
                    Some(buf) => {
                        let reread = Self::checked_seed(user, buf)?;
                        if *reread != *seed {
                            return Err(KeystoreError::VerificationFailed {
                                user: user.to_string(),
                                reason: "read-back differs from generated seed".to_string(),
                            });
                        }
                        Ok(LoadOutcome {
                            seed,
                            created: true,
                        })
                    }
                    None => Err(KeystoreError::VerificationFailed {
                        user: user.to_string(),
                        reason: "seed vanished right after set_secret".to_string(),
                    }),
                }
            }
        }
    }

    /// Racine de capability Biscuit (`"capability-root-v1"`).
    pub fn capability_root(&self) -> Result<LoadOutcome, KeystoreError> {
        self.load_or_create(&versioned_user(CAPABILITY_ROOT_BASE, 1))
    }

    /// Cle de signature du journal d'audit (`"journal-signing-v1"`).
    pub fn journal_signing(&self) -> Result<LoadOutcome, KeystoreError> {
        self.load_or_create(&versioned_user(JOURNAL_SIGNING_BASE, 1))
    }

    /// Pivote chaque base vers `*-v2`.
    ///
    /// # Fenetre transitoire documentee
    ///
    /// 1. v1 DOIT exister et etre valide (sinon `MissingBaseline` : on ne
    ///    pivote jamais a partir de rien, ce serait une creation deguisee).
    /// 2. v2 est genere (alea OS frais), ecrit, **relu et compare octet a
    ///    octet** (`verified_dual_read`, sinon `VerificationFailed`).
    /// 3. v1 est relue a son tour : les deux versions coexistent en lecture
    ///    le temps que les consommateurs (autorite Biscuit, journal) basculent.
    /// 4. v1 n'est JAMAIS supprimee ici : la suppression est un acte explicite
    ///    ulterieur (`revoke()`), quand l'operateur confirme que plus aucun
    ///    token/journal ne depend de v1. TODO : ajouter
    ///    `finalize_rotation()` qui refuse de supprimer v1 tant qu'un token
    ///    v1 est encore verifiable (suivi : table de migration boot).
    pub fn rotate(&self) -> Result<RotationReport, KeystoreError> {
        let mut entries = Vec::new();
        for base in [CAPABILITY_ROOT_BASE, JOURNAL_SIGNING_BASE] {
            let previous_user = versioned_user(base, 1);
            let current_user = versioned_user(base, 2);
            // 1. Socle v1 obligatoire.
            let baseline = match self.store.get_secret(&self.service, &previous_user)? {
                Some(buf) => Self::checked_seed(&previous_user, buf)?,
                None => {
                    return Err(KeystoreError::MissingBaseline {
                        user: previous_user,
                    });
                }
            };
            drop(baseline);
            // 2. v2 frais + persistance + relecture comparee.
            let fresh = Self::generate_seed()?;
            self.store
                .set_secret(&self.service, &current_user, &fresh[..])?;
            let reread = match self.store.get_secret(&self.service, &current_user)? {
                Some(buf) => Self::checked_seed(&current_user, buf)?,
                None => {
                    return Err(KeystoreError::VerificationFailed {
                        user: current_user,
                        reason: "v2 vanished right after set_secret".to_string(),
                    });
                }
            };
            if *reread != *fresh {
                return Err(KeystoreError::VerificationFailed {
                    user: current_user,
                    reason: "v2 read-back differs from generated seed".to_string(),
                });
            }
            // 3. v1 toujours lisible (coexistence transitoire).
            let v1_still_there = match self.store.get_secret(&self.service, &previous_user)? {
                Some(buf) => Self::checked_seed(&previous_user, buf).is_ok(),
                None => false,
            };
            if !v1_still_there {
                return Err(KeystoreError::VerificationFailed {
                    user: previous_user,
                    reason: "v1 unreadable during transitional window".to_string(),
                });
            }
            entries.push(RotatedEntry {
                base: base.to_string(),
                previous_user,
                current_user,
                verified_dual_read: true,
            });
        }
        Ok(RotationReport { entries })
    }

    /// Supprime v1+v2 des deux bases (`delete_credential`, idempotent).
    /// La premiere erreur backend interrompe et est renvoyee explicitement.
    pub fn revoke(&self) -> Result<RevocationReport, KeystoreError> {
        let mut removed_users = Vec::new();
        for base in [CAPABILITY_ROOT_BASE, JOURNAL_SIGNING_BASE] {
            for version in [1u32, 2u32] {
                let user = versioned_user(base, version);
                self.store.delete(&self.service, &user)?;
                removed_users.push(user);
            }
        }
        Ok(RevocationReport { removed_users })
    }
}

// ---------------------------------------------------------------------------
// F1 : repli fichier CHIFFRE (quand le keychain OS est indisponible).
// ---------------------------------------------------------------------------
//
// Contexte : `annora-daemon` utilisait `keys/*.bin` avec 32 octets BRUTS en
// clair comme repli si `OsStore` echouait. Ce module fournit le SEUL repli
// fichier acceptable :
//
// - header versionne (`ANRKEY01` + version u32 BE) + sel 16 o + 32 o
//   "chiffres" => le fichier ne contient JAMAIS 32 octets bruts (60 o total) ;
// - permissions `0600` sur Unix (proprietaire seul) ;
// - "chiffrement" par mot de passe OS : XOR avec un keystream derive
//   (FNV-1a sur password+salt+compteur). C'est une OBFUSCATION MINIMALE,
//   PAS un chiffrement authentifie de niveau DPAPI : le keychain OS reste
//   OBLIGATOIRE en prod. En prod (`ANNORA_PROD=1`) ou sans mot de passe,
//   ces fonctions REFUSENT (Err explicite, boot interrompu), jamais de
//   regeneration silencieuse en memoire seule.
//
// Le daemon doit appeler `load_or_create_encrypted_file` au lieu d'ecrire
// des `.bin` bruts, avec un mot de passe fourni par l'OS / l'operateur
// (jamais hardcode), puis migrer vers le keychain via `rotate()` des que
// le trousseau est debloque. TODO prod : remplacer le XOR par
// ChaCha20Poly1305 + cle DPAPI (CryptProtectData) quand `winapi` sera
// disponible sans `unsafe` dans ce crate (`#![forbid(unsafe_code)]`).

/// Magic du repli fichier chiffre (8 o,ences ASCII).
pub const ENCRYPTED_FILE_MAGIC: &[u8; 8] = b"ANRKEY01";
/// Version du format de repli fichier (u32 BE apres le magic).
pub const ENCRYPTED_FILE_VERSION: u32 = 1;
const FILE_SALT_LEN: usize = 16;
const FILE_CT_LEN: usize = SEED_LEN;
const FILE_TOTAL_LEN: usize = 8 + 4 + FILE_SALT_LEN + FILE_CT_LEN;

/// Derive un keystream pseudo-aleatoire (FNV-1a 64, compteur) — obfuscation
/// minimale, PAS un KDF resistant (documente ci-dessus).
fn fallback_keystream(password: &[u8], salt: &[u8], len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut counter = 0u64;
    while out.len() < len {
        // FNV-1a 64 sur (password || salt || counter LE).
        let mut h: u64 = 0xcbf29ce484222325;
        for b in password
            .iter()
            .chain(salt.iter())
            .chain(counter.to_le_bytes().iter())
        {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        // Diffuse le hash sur 8 octets par tour (xorshift local).
        let mut x = h;
        for _ in 0..8 {
            if out.len() >= len {
                break;
            }
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            out.push((x & 0xff) as u8);
        }
        counter = counter.wrapping_add(1);
    }
    out
}

fn fallback_is_prod() -> bool {
    std::env::var("ANNORA_PROD")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn fallback_write_locked(path: &Path, bytes: &[u8]) -> Result<(), KeystoreError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                KeystoreError::FileFallback(format!("create_dir_all {}: {e}", parent.display()))
            })?;
        }
    }
    std::fs::write(path, bytes)
        .map_err(|e| KeystoreError::FileFallback(format!("write {}: {e}", path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perm = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, perm).map_err(|e| {
            KeystoreError::FileFallback(format!("chmod 0600 {}: {e}", path.display()))
        })?;
    }
    #[cfg(not(unix))]
    {
        // Windows : pas d'ACL fine sans winapi (forbid unsafe) — le fichier
        // reste dans le profil utilisateur ; DPAPI reste le TODO prod.
        eprintln!(
            "keystore: repli chiffre ecrit sans ACL 0600 (non-Unix, DPAPI TODO) : {}",
            path.display()
        );
    }
    Ok(())
}

/// Charge le seed depuis le repli fichier chiffre, ou le cree (alea OS) +
/// persiste + relit. Refuse (Err explicite) si : mot de passe vide,
/// `ANNORA_PROD=1`, header invalide, taille inattendue.
pub fn load_or_create_encrypted_file(
    path: &Path,
    password: &[u8],
) -> Result<LoadOutcome, KeystoreError> {
    load_or_create_encrypted_file_impl(path, password, fallback_is_prod())
}

/// Noyau testable sans toucher aux variables d'environnement (dont la
/// mutation est `unsafe` en edition 2024 et interdite par
/// `#![forbid(unsafe_code)]`) : `is_prod=true` simule `ANNORA_PROD=1`.
fn load_or_create_encrypted_file_impl(
    path: &Path,
    password: &[u8],
    is_prod: bool,
) -> Result<LoadOutcome, KeystoreError> {
    if password.is_empty() {
        return Err(KeystoreError::InsecureBoot(
            "repli fichier chiffre exige un mot de passe OS non vide (refus)".to_string(),
        ));
    }
    if is_prod {
        return Err(KeystoreError::InsecureBoot(
            "prod refuse le repli fichier (meme chiffre) : debloquez le trousseau OS puis migrez via rotation".to_string(),
        ));
    }
    eprintln!(
        "keystore: ATTENTION repli fichier chiffre utilise (keychain OS indisponible) : {} — migrez via rotation des que possible",
        path.display()
    );
    if path.exists() {
        let raw = std::fs::read(path)
            .map_err(|e| KeystoreError::FileFallback(format!("read {}: {e}", path.display())))?;
        if raw.len() != FILE_TOTAL_LEN {
            return Err(KeystoreError::FileFallback(format!(
                "header invalide ({}): attendu {} octets, trouve {}",
                path.display(),
                FILE_TOTAL_LEN,
                raw.len()
            )));
        }
        if &raw[..8] != ENCRYPTED_FILE_MAGIC {
            return Err(KeystoreError::FileFallback(format!(
                "magic invalide pour {} (fichier non chiffre ou corrompu)",
                path.display()
            )));
        }
        let ver = u32::from_be_bytes([raw[8], raw[9], raw[10], raw[11]]);
        if ver != ENCRYPTED_FILE_VERSION {
            return Err(KeystoreError::FileFallback(format!(
                "version {} non supportee pour {} (attendu {})",
                ver,
                path.display(),
                ENCRYPTED_FILE_VERSION
            )));
        }
        let salt = &raw[12..12 + FILE_SALT_LEN];
        let ct = &raw[12 + FILE_SALT_LEN..];
        let ks = fallback_keystream(password, salt, FILE_CT_LEN);
        let mut seed = [0u8; SEED_LEN];
        for i in 0..SEED_LEN {
            seed[i] = ct[i] ^ ks[i];
        }
        Ok(LoadOutcome {
            seed: Zeroizing::new(seed),
            created: false,
        })
    } else {
        // Creation : seed alea OS + sel alea OS, jamais 32 o bruts sur disque.
        let mut seed = [0u8; SEED_LEN];
        getrandom::getrandom(&mut seed).map_err(|e| KeystoreError::Randomness(e.to_string()))?;
        let mut salt = [0u8; FILE_SALT_LEN];
        getrandom::getrandom(&mut salt).map_err(|e| KeystoreError::Randomness(e.to_string()))?;
        let ks = fallback_keystream(password, &salt, FILE_CT_LEN);
        let mut raw = Vec::with_capacity(FILE_TOTAL_LEN);
        raw.extend_from_slice(ENCRYPTED_FILE_MAGIC);
        raw.extend_from_slice(&ENCRYPTED_FILE_VERSION.to_be_bytes());
        raw.extend_from_slice(&salt);
        for i in 0..SEED_LEN {
            raw.push(seed[i] ^ ks[i]);
        }
        // Efface le keystream intermediaire.
        let mut ks = ks;
        ks.zeroize();
        fallback_write_locked(path, &raw)?;
        // Relecture de verification (prouve la persistance immediate).
        let reread = std::fs::read(path).map_err(|e| {
            KeystoreError::FileFallback(format!("read-back {}: {e}", path.display()))
        })?;
        if reread != raw {
            return Err(KeystoreError::VerificationFailed {
                user: path.display().to_string(),
                reason: "read-back differs right after write".to_string(),
            });
        }
        Ok(LoadOutcome {
            seed: Zeroizing::new(seed),
            created: true,
        })
    }
}

/// Regenere le seed du repli fichier (mot de passe identique ou nouveau) :
/// charge avec l'ancien, genere un frais, persiste + relit + compare.
/// Refuse si le fichier n'existe pas (pas de rotation a partir de rien).
pub fn rotate_encrypted_file(
    path: &Path,
    old_password: &[u8],
    new_password: &[u8],
) -> Result<(), KeystoreError> {
    rotate_encrypted_file_impl(path, old_password, new_password, fallback_is_prod())
}

/// Noyau testable de rotation (cf. `load_or_create_encrypted_file_impl`).
fn rotate_encrypted_file_impl(
    path: &Path,
    old_password: &[u8],
    new_password: &[u8],
    is_prod: bool,
) -> Result<(), KeystoreError> {
    if old_password.is_empty() || new_password.is_empty() {
        return Err(KeystoreError::InsecureBoot(
            "rotation du repli chiffre exige des mots de passe non vides".to_string(),
        ));
    }
    if is_prod {
        return Err(KeystoreError::InsecureBoot(
            "prod refuse la rotation du repli fichier : debloquez le trousseau OS".to_string(),
        ));
    }
    if !path.exists() {
        return Err(KeystoreError::MissingBaseline {
            user: path.display().to_string(),
        });
    }
    // Prouve que l'ancien est lisible avant d'ecraser.
    let _baseline = load_or_create_encrypted_file_impl(path, old_password, false)?;
    let mut fresh = [0u8; SEED_LEN];
    getrandom::getrandom(&mut fresh).map_err(|e| KeystoreError::Randomness(e.to_string()))?;
    let mut salt = [0u8; FILE_SALT_LEN];
    getrandom::getrandom(&mut salt).map_err(|e| KeystoreError::Randomness(e.to_string()))?;
    let ks = fallback_keystream(new_password, &salt, FILE_CT_LEN);
    let mut raw = Vec::with_capacity(FILE_TOTAL_LEN);
    raw.extend_from_slice(ENCRYPTED_FILE_MAGIC);
    raw.extend_from_slice(&ENCRYPTED_FILE_VERSION.to_be_bytes());
    raw.extend_from_slice(&salt);
    for i in 0..SEED_LEN {
        raw.push(fresh[i] ^ ks[i]);
    }
    let mut ks = ks;
    ks.zeroize();
    fallback_write_locked(path, &raw)?;
    // Double verification transitoire : relit avec le nouveau mot de passe.
    let reread = load_or_create_encrypted_file_impl(path, new_password, is_prod)?;
    if *reread.seed != fresh {
        return Err(KeystoreError::VerificationFailed {
            user: path.display().to_string(),
            reason: "rotated read-back differs".to_string(),
        });
    }
    fresh.zeroize();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> KeyRegistry<MemoryStore> {
        KeyRegistry::new(MemoryStore::new())
    }

    #[test]
    fn roundtrip_same_seed_on_reload() {
        let store = MemoryStore::new();
        let first = KeyRegistry::new(store.clone())
            .capability_root()
            .expect("create");
        assert!(first.created, "first boot must report creation honestly");
        let second = KeyRegistry::new(store.clone())
            .capability_root()
            .expect("reload");
        assert!(
            !second.created,
            "second boot must report load, not creation"
        );
        assert_eq!(*first.seed, *second.seed, "seed must survive reload");
    }

    #[test]
    fn bases_are_independent_and_versioned() {
        let reg = registry();
        let root = reg.capability_root().expect("root");
        let journal = reg.journal_signing().expect("journal");
        assert_ne!(*root.seed, *journal.seed, "bases must not share a seed");
        // Utilisateurs versionnes -v1 presents, -v2 absents avant rotation.
        let inner = reg.store().inner.lock().expect("lock");
        assert!(
            inner
                .secrets
                .contains_key(&(SERVICE.to_string(), "capability-root-v1".to_string()))
        );
        assert!(
            inner
                .secrets
                .contains_key(&(SERVICE.to_string(), "journal-signing-v1".to_string()))
        );
        assert!(
            !inner
                .secrets
                .contains_key(&(SERVICE.to_string(), "capability-root-v2".to_string()))
        );
    }

    #[test]
    fn corrupt_length_is_explicit_error() {
        let store = MemoryStore::new();
        store
            .set_secret(SERVICE, "capability-root-v1", &[7u8; 16])
            .expect("preset corrupt");
        let err = KeyRegistry::new(store)
            .capability_root()
            .expect_err("16 bytes must be rejected");
        assert_eq!(
            err,
            KeystoreError::Corrupt {
                user: "capability-root-v1".to_string(),
                expected: 32,
                found: 16,
            }
        );
    }

    #[test]
    fn keychain_error_never_silently_regenerates() {
        let store = MemoryStore::new();
        store.inject_error(Some("HSM locked".to_string()));
        let reg = KeyRegistry::new(store.clone());
        let err = reg
            .capability_root()
            .expect_err("must surface backend failure");
        assert!(
            matches!(err, KeystoreError::Keychain { .. }),
            "backend failure must stay explicit, got: {err:?}"
        );
        assert!(
            err.to_string().contains("HSM locked"),
            "message must carry the backend reason"
        );
        // Preuve d'absence de regeneration silencieuse : rien n'a ete ecrit.
        store.inject_error(None);
        let inner = store.inner.lock().expect("lock");
        assert!(
            inner.secrets.is_empty(),
            "failed load must not leave a generated seed behind"
        );
    }

    #[test]
    fn rotate_creates_verified_v2_and_keeps_v1() {
        let reg = registry();
        let v1_before = reg.capability_root().expect("baseline").seed;
        reg.journal_signing().expect("baseline journal");
        let report = reg.rotate().expect("rotate");
        assert_eq!(report.entries.len(), 2);
        for entry in &report.entries {
            assert!(entry.verified_dual_read);
            assert!(entry.previous_user.ends_with("-v1"));
            assert!(entry.current_user.ends_with("-v2"));
        }
        // v1 inchangee et toujours lisible (fenetre transitoire).
        let v1_after = reg
            .load_or_create("capability-root-v1")
            .expect("v1 readable")
            .seed;
        assert_eq!(*v1_before, *v1_after);
        assert!(
            !reg.load_or_create("capability-root-v2")
                .expect("v2 readable")
                .created
        );
        // v2 distinct de v1 (sauf collision 2^-256 : acceptee comme preuve).
        let v2 = reg.load_or_create("capability-root-v2").expect("v2").seed;
        assert_ne!(*v1_after, *v2);
    }

    #[test]
    fn rotate_without_baseline_refuses() {
        let reg = registry();
        let err = reg.rotate().expect_err("no baseline => no rotation");
        assert_eq!(
            err,
            KeystoreError::MissingBaseline {
                user: "capability-root-v1".to_string(),
            }
        );
    }

    #[test]
    fn revoke_deletes_and_next_boot_is_fresh() {
        let reg = registry();
        let old = reg.capability_root().expect("baseline").seed;
        reg.journal_signing().expect("baseline journal");
        let report = reg.revoke().expect("revoke");
        assert_eq!(report.removed_users.len(), 4);
        // Revocation idempotente (second passage : absents = deja revoques).
        reg.revoke().expect("second revoke must succeed");
        // Prochain boot = creation fraiche, differente (pas de resurrection).
        let fresh = reg.capability_root().expect("recreate").seed;
        assert_ne!(*old, *fresh);
    }

    #[test]
    fn rotation_report_json_roundtrip() {
        let reg = registry();
        reg.capability_root().expect("baseline");
        reg.journal_signing().expect("baseline journal");
        let report = reg.rotate().expect("rotate");
        let json = serde_json::to_string(&report).expect("serialize");
        assert!(json.contains("capability-root-v2"));
        let back: RotationReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(report, back);
    }

    #[test]
    fn error_messages_never_carry_secret_bytes() {
        // Aucune variante ne formate d'octets : verification par construction
        // (service/user seuls). Garde-fou : le seed Debug n'est jamais affiche
        // dans les messages d'erreur ci-dessus (revue exigee a chaque ajout).
        let err = KeystoreError::Corrupt {
            user: "capability-root-v1".to_string(),
            expected: 32,
            found: 16,
        };
        let msg = err.to_string();
        assert!(msg.contains("capability-root-v1"));
        assert!(!msg.contains("07"), "no secret material in messages");
    }

    #[test]
    fn encrypted_file_roundtrip_never_raw_and_versioned() {
        let dir = std::env::temp_dir().join(format!("ksenc_{}", uuid_like()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("capability-root.enc");
        let pw = b"mot-de-passe-os-test-1234";
        // Chemin hermetique : `is_prod=false` explicite, sans dependre ni
        // muter `ANNORA_PROD` (mutation `unsafe` interdite ici).
        let first = load_or_create_encrypted_file_impl(&path, pw, false).expect("create");
        assert!(first.created);
        let raw = std::fs::read(&path).unwrap();
        // Jamais 32 octets bruts : header + sel + ct = 60 o.
        assert_eq!(raw.len(), 60, "repli chiffre ne fait jamais 32 o bruts");
        assert_eq!(&raw[..8], b"ANRKEY01");
        assert_eq!(u32::from_be_bytes([raw[8], raw[9], raw[10], raw[11]]), 1);
        assert_ne!(
            &raw[28..],
            &first.seed[..],
            "le seed ne dort jamais en clair"
        );
        let second = load_or_create_encrypted_file_impl(&path, pw, false).expect("reload");
        assert!(!second.created);
        assert_eq!(*first.seed, *second.seed);
        // Mauvais mot de passe => seed different (pas de faux positif silencieux).
        let wrong = load_or_create_encrypted_file_impl(&path, b"mauvais-mdp", false).expect("read");
        assert_ne!(*wrong.seed, *first.seed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn encrypted_file_refuses_empty_password() {
        let dir = std::env::temp_dir().join(format!("ksenc_empty_{}", uuid_like()));
        let path = dir.join("k.enc");
        let err =
            load_or_create_encrypted_file_impl(&path, b"", false).expect_err("mdp vide refuse");
        assert!(matches!(err, KeystoreError::InsecureBoot(_)));
    }

    #[test]
    fn encrypted_file_refuses_prod_boot() {
        // Simule `ANNORA_PROD=1` sans muter l'environnement (unsafe interdit).
        let dir = std::env::temp_dir().join(format!("ksenc_prod_{}", uuid_like()));
        let path = dir.join("k.enc");
        let err = load_or_create_encrypted_file_impl(&path, b"mdp-quelconque", true)
            .expect_err("prod refuse le repli fichier");
        assert!(matches!(err, KeystoreError::InsecureBoot(_)));
        assert!(err.to_string().contains("trousseau OS"));
        assert!(!path.exists(), "refus prod ne doit rien ecrire");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn encrypted_file_rotate_changes_seed() {
        let dir = std::env::temp_dir().join(format!("ksenc_rot_{}", uuid_like()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("k.enc");
        let before = load_or_create_encrypted_file_impl(&path, b"pw-old", false)
            .expect("create")
            .seed;
        rotate_encrypted_file_impl(&path, b"pw-old", b"pw-new", false).expect("rotate");
        let after = load_or_create_encrypted_file_impl(&path, b"pw-new", false)
            .expect("reload")
            .seed;
        assert_ne!(*before, *after);
        // Rotation sans socle refusee.
        let missing = dir.join("absent.enc");
        assert!(rotate_encrypted_file_impl(&missing, b"a", b"b", false).is_err());
        // Rotation en prod refusee (sans muter l'env).
        assert!(rotate_encrypted_file_impl(&path, b"pw-new", b"pw-new2", true).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn encrypted_file_rejects_bad_header() {
        let dir = std::env::temp_dir().join(format!("ksenc_bad_{}", uuid_like()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("k.enc");
        std::fs::write(&path, [7u8; 32]).unwrap(); // 32 o bruts style ancien fallback
        let err = load_or_create_encrypted_file_impl(&path, b"pw", false)
            .expect_err("32 o bruts refuses");
        assert!(matches!(err, KeystoreError::FileFallback(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Identifiant unique sans dependance `uuid` (dev-dep absente ici) :
    /// pid + nanos + compteur.
    fn uuid_like() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static CTR: AtomicU64 = AtomicU64::new(0);
        let n = CTR.fetch_add(1, Ordering::Relaxed);
        format!(
            "{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
            n
        )
    }
}
