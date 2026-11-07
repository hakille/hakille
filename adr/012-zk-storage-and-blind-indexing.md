# ADR 012: Stockage Zero-Knowledge, Indexation Aveugle et Crémation Cryptographique

## Statut
Accepté (Mis à jour pour la version souveraine de Hakille)

## Contexte
Hakille est un système d'exploitation cognitif local-first et souverain (Rust pur, MSRV 1.95, édition 2024, zéro runtime Python). L'architecture de persistance locale repose sur des moteurs embarqués légers et performants (`redb`, SQLite déterministe). Cependant, la persistance sur disque d'artefacts cognitifs hautement sensibles — souvenirs épisodiques, identifiants personnels, jetons de capacités, gabarits biométriques et graphes de relations — soulève des défis de sécurité critiques :

1. **Menace de compromission physique du disque ou de saisie judiciaire** : En cas d'accès direct aux fichiers de stockage par un attaquant ou un logiciel espion hôte, aucun enregistrement ne doit être lisible en clair (*Zero-Knowledge at Rest*).
2. **Fuite de métadonnées et attaques par analyse de fréquence** : L'utilisation d'index textuels ordinaires révèle la présence et la fréquence de termes recherchés (e.g. adresses e-mail, noms de contacts, mots-clés confidentiels).
3. **Attaques par découpage-collage inter-tables et inter-tenants (*Ciphertext Splicing*)** : Sans liaison cryptographique contextuelle, un attaquant disposant des droits d'écriture sur le disque pourrait substituer un bloc chiffré valide d'une table à une autre, ou transférer un secret d'un tenant familial vers un tenant d'entreprise.
4. **Droit à l'oubli et crémation cryptographique instantanée** : Les exigences du RGPD (Art. 17) et le protocole de souveraineté exigent qu'un utilisateur puisse détruire l'intégralité de ses données en quelques microsecondes sans avoir à écraser gigaoctet par gigaoctet un SSD soumis à l'usure (*wear-leveling*).
5. **Récupération sociale sans tiers centralisé** : La perte de mot de passe ne doit pas entraîner la perte irrémédiable de l'identité, sans pour autant introduire un serveur central de réinitialisation.

---

## Architecture Globale du Stockage Zero-Knowledge

```
+----------------------------------------------------------------------------------------------------+
|                                HAKILLE SECURE ENCLAVE / RAM ONLY                                   |
|                                                                                                    |
|   +--------------------+     Argon2id (RFC 9106)      +----------------------------------------+   |
|   |  Passphrase Maître | ---------------------------> | K_master (32 octets, ZeroizeOnDrop)    |   |
|   +--------------------+  64 MiB, t=3 passes, p=4     +----------------------------------------+   |
|                                                                    |                               |
|                                                                    v HKDF-Expand (RFC 5869)        |
|   +--------------------------------------------------------------------------------------------+   |
|   |                              SubkeyHierarchy (ZeroizeOnDrop)                               |   |
|   |                                                                                            |   |
|   |  * K_id      = HKDF(K_master, "hakille:subkeys:v1:identity")        -> Clé Profil / Identité|   |
|   |  * K_mem     = HKDF(K_master, "hakille:subkeys:v1:memory-partition")-> Clé Mémoire cognitive |   |
|   |  * K_cred    = HKDF(K_master, "hakille:subkeys:v1:credentials")     -> Clé Jetons & Secret   |   |
|   |  * K_bio     = HKDF(K_master, "hakille:subkeys:v1:biometrics")      -> Clé Projection Bio    |   |
|   |  * K_bindex  = HKDF(K_master, "hakille:subkeys:v1:blind-index")     -> Clé Index Aveugle     |   |
|   +--------------------------------------------------------------------------------------------+   |
+----------------------------------------------------------------------------------------------------+
                                               |
                  +----------------------------+----------------------------+
                  |                                                         |
                  v                                                         v
   +-------------------------------+                         +-------------------------------+
   |   Recherche par Index Aveugle |                         |    Chiffrement Authentifié    |
   |                               |                         |       XChaCha20-Poly1305      |
   |   Terme en clair (e.g. Email) |                         |      Plaintext Record         |
   |              |                |                         |              |                |
   |              v                |                         |              v                |
   |   Normalisation Unicode NFKC  |                         |    AAD Binding Deterministe   |
   |              |                |                         |   "hakille:aad:v1:<tenant>... |
   |              v                |                         |              |                |
   |   HMAC-SHA256(K_bindex, ...)  |                         |              v                |
   |              |                |                         |  Nonce 192-bit + Tag 128-bit  |
   |              v                |                         |              |                |
   |     Token Aveugle 256-bit     |                         |              v                |
   +-------------------------------+                         +-------------------------------+
                  |                                                         |
                  +----------------------------+----------------------------+
                                               |
                                               v
+----------------------------------------------------------------------------------------------------+
|                              BASE DE DONNÉES DISQUE EMBARQUÉE (redb)                               |
|                                                                                                    |
|   Table: BLIND_INDEX_MAP                                                                           |
|   +----------------------------------------------------+---------------------------------------+   |
|   | Exact Token (HMAC 32 octets)                       | Vec<Record UUID>                      |   |
|   | Prefix Token (HMAC 32 octets, len=4..8)            | Vec<Record UUID>                      |   |
|   +----------------------------------------------------+---------------------------------------+   |
|                                                                                                    |
|   Table: ENCRYPTED_RECORDS                                                                         |
|   +-----------------------+--------------------------------------------------------------------+   |
|   | Record UUID           | XChaChaBlob { nonce: [u8;24], ciphertext, tag: [u8;16], aad_fp }   |   |
|   +-----------------------+--------------------------------------------------------------------+   |
|                                                                                                    |
|   Table: RECOVERY_SHARES                                                                           |
|   +-----------------------+--------------------------------------------------------------------+   |
|   | Share UUID            | SecretShare { index: u8, data: [u8;32], checksum: [u8;4] }         |   |
|   +-----------------------+--------------------------------------------------------------------+   |
+----------------------------------------------------------------------------------------------------+
```

---

## Formulations Mathématiques et Cryptographiques

### 1. Dérivation de Clé Maître et Séparation de Domaines

La clé maître $K_{\text{master}}$ est dérivée de la passphrase utilisateur $P$ et d'un sel cryptographique $S \in \{0, 1\}^{256}$ généré par le générateur aléatoire du système d'exploitation (`OsRng`) conformément au RFC 9106 :

$$K_{\text{master}} = \text{Argon2id}(P, S, m=65536\,\text{KiB}, t=3, p=4, L=32)$$

L'arbre des sous-clés spécialisées est ensuite généré via le mécanisme HKDF (RFC 5869) :

$$\text{PRK} = \text{HKDF-Extract}(\text{salt}=\mathbf{0}^{256}, \text{IKM}=K_{\text{master}})$$

Pour chaque domaine fonctionnel $D \in \{\text{"identity"}, \text{"memory-partition"}, \text{"credentials"}, \text{"biometrics"}, \text{"blind-index"}\}$ :

$$K_{D} = \text{HKDF-Expand}(\text{PRK}, \text{"hakille:subkeys:v1:"} \parallel D, L=32)$$

Cette séparation stricte garantit que même si une sous-clé sectorielle venait à être compromise en mémoire, elle ne permettrait ni de dériver les autres sous-clés, ni de remonter à $K_{\text{master}}$.

### 2. Chiffrement Symétrique Authentifié XChaCha20-Poly1305 avec Liaison AAD

Le chiffrement repose sur la construction XChaCha20-Poly1305 avec nonces étendus de 192 bits (24 octets). Cette longueur de nonce autorise la génération aléatoire sans aucun risque de collision d'anniversaire ($2^{96}$ nonces avant 50% de probabilité de collision).

Soit un message clair $M$, une sous-clé $K \in \{0, 1\}^{256}$, et un nonce $N \in \{0, 1\}^{192}$.
Le nonce est partitionné en $N = N_0 \parallel N_1$ où $N_0 \in \{0, 1\}^{128}$ et $N_1 \in \{0, 1\}^{64}$.

1. **Dérivation de sous-clé intermédiaire** via le bloc de permutation HChaCha20 :
   $$K_{\text{sub}} = \text{HChaCha20}(K, N_0)$$
2. **Génération de la clé d'authentification Poly1305 à usage unique** :
   $$R \parallel S = \text{ChaCha20}(K_{\text{sub}}, \text{counter}=0, \text{nonce}=\mathbf{0}^{32} \parallel N_1)[0..64]$$
3. **Chiffrement du flux** :
   $$C = M \oplus \text{ChaCha20}(K_{\text{sub}}, \text{counter}=1, \text{nonce}=\mathbf{0}^{32} \parallel N_1)$$
4. **Calcul du tag d'authentification Poly1305** sur les Données Authentifiées Additionnelles (AAD) et le ciphertext $C$ :
   $$T = \text{Poly1305}_{R, S}(\text{pad16}(\text{AAD}) \parallel \text{pad16}(C) \parallel \text{len}(\text{AAD})_8 \parallel \text{len}(C)_8)$$

#### Structure Canonique du Contexte AAD
Pour éradiquer les attaques par découpage-collage inter-tables ou inter-tenants (*ciphertext splicing attacks*), l'AAD est formulé de manière déterministe avec le délimiteur standard ASCII Unit Separator (`0x1F`) :

$$\text{AAD} = \text{"hakille:aad:v1:"} \parallel \text{tenant\_id} \parallel \mathtt{0x1F} \parallel \text{user\_id} \parallel \mathtt{0x1F} \parallel \text{table\_id} \parallel \mathtt{0x1F} \parallel \text{record\_uuid} \parallel \mathtt{0x1F} \parallel \text{schema\_version}_4$$

L'empreinte $\text{Fingerprint}_{\text{AAD}} = \text{SHA-256}(\text{AAD})$ est stockée dans le blob chiffré pour accélérer le diagnostic d'intégrité avant déchiffrement complet.

### 3. Indexation Aveugle Déterministe (HMAC-SHA256 Blind Indexing)

Pour permettre la recherche exacte et par préfixe sur disque sans déchiffrer les données ni révéler les termes de recherche, Hakille utilise un schéma d'indexation aveugle :

1. **Normalisation Unicode stricte** :
   Le terme en clair $w$ subit un nettoyage rigoureux : suppression des espaces de début et de fin, passage en minuscules Unicode, et réduction des espaces contigus :
   $$\bar{w} = \text{Normalize}(w)$$
2. **Jeton d'Indexation Aveugle Exacte** :
   $$T_{\text{exact}}(\bar{w}) = \text{HMAC-SHA256}(K_{\text{bindex}}, \text{"hakille:blind:exact:v1:"} \parallel \bar{w})$$
3. **Jeton d'Indexation Aveugle par Préfixe** :
   Pour un préfixe de longueur $\ell \in [3, 8]$ caractères :
   $$T_{\text{prefix}}(\bar{w}, \ell) = \text{HMAC-SHA256}(K_{\text{bindex}}, \text{"hakille:blind:prefix:v1:"} \parallel \text{to\_le\_bytes}(\ell) \parallel \text{":"} \parallel \text{slice}(\bar{w}, 0, \ell))$$

La recherche d'un terme $q$ s'exécute ainsi : le client calcule $T_{\text{exact}}(\text{Normalize}(q))$ en mémoire avec sa clé secrète $K_{\text{bindex}}$, puis interroge la table B-Tree de `redb` pour obtenir la liste des UUIDs correspondants en temps $O(\log N)$. Les enregistrements pointés sont alors chargés et déchiffrés. Un attaquant observant la base de données ne voit que des condensats pseudo-aléatoires de 256 bits impossibles à inverser sans $K_{\text{bindex}}$.

### 4. Partage de Secret de Shamir dans le Corps de Galois $GF(2^8)$

Pour la récupération sociale ($k$-parmi-$n$) de la clé maître ou des secrets critiques sans tiers centralisé, Hakille implémente le schéma de Shamir dans le corps fini $GF(2^8) = \mathbb{F}_2[x] / (x^8 + x^4 + x^3 + x + 1)$ (polynôme irréductible `0x11b`).

Soit le secret $S \in \{0, 1\}^{256}$ partitionné en 32 octets indépendants $s^{(b)}$ pour $b \in [0, 31]$.
Pour chaque octet $s^{(b)}$, un polynôme aléatoire de degré $k - 1$ est généré :

$$f^{(b)}(x) = s^{(b)} \oplus \bigoplus_{j=1}^{k-1} a_j^{(b)} \otimes x^j$$

où $\oplus$ et $\otimes$ désignent respectivement l'addition (XOR) et la multiplication polynomiale modulo `0x11b` dans $GF(2^8)$.

Chaque partiteur $i \in [1, n]$ reçoit la part :

$$P_i = \left( i, \; \left[ f^{(0)}(i), f^{(1)}(i), \dots, f^{(31)}(i) \right], \; \text{Checksum}_i \right)$$

avec un checksum d'intégrité de 4 octets :

$$\text{Checksum}_i = \text{SHA-256}(\text{"hakille:shamir:v1:"} \parallel i \parallel \text{Data}_i)[0..4]$$

La reconstruction à partir d'un sous-ensemble quelconque de $k$ parts distinctes $\{P_{i_1}, \dots, P_{i_k}\}$ s'effectue par interpolation de Lagrange en $x = 0$ :

$$s^{(b)} = f^{(b)}(0) = \bigoplus_{j=1}^{k} f^{(b)}(i_j) \otimes \ell_j(0)$$

où les coefficients de base de Lagrange sont calculés dans $GF(2^8)$ :

$$\ell_j(0) = \bigotimes_{m \ne j} \frac{i_m}{i_m \oplus i_j}$$

### 5. Extracteur Flou (*Fuzzy Extractor*) pour Gabarits Biométriques

Les représentations biométriques sont soumises au bruit intrinsèque des capteurs et des expressions faciales. Un extracteur flou basé sur la construction *code-offset* convertit une empreinte continue bruitée $w$ en une chaîne binaire cryptographique reproductible $R \in \{0, 1\}^{256}$.

1. **Génération (Enrôlement)** :
   - Sélection uniforme d'une clé secrète $R \in \{0, 1\}^{256}$.
   - Encodage de $R$ en un mot de code $C_R \in \{0, 1\}^{7936}$ via un code correcteur d'erreurs par répétition entrelacée de facteur $r=31$.
   - Binarisation du vecteur biométrique 512D en un gabarit $w \in \{0, 1\}^{7936}$ par projections de signes d'hyperplans pseudo-aléatoires.
   - Calcul des données d'aide publiques (*Helper Data*) :
     $$P = w \oplus C_R$$
   - Calcul de l'engagement cryptographique public :
     $$C = \text{SHA-256}(R \parallel \text{Salt})$$
2. **Reproduction (Vérification)** :
   - Présentation d'un gabarit de sonde $w' \in \{0, 1\}^{7936}$ issu d'une nouvelle capture.
   - Soustraction des données d'aide :
     $$C'_R = w' \oplus P = w' \oplus (w \oplus C_R) = C_R \oplus (w \oplus w')$$
   - Décodage à vote majoritaire sur les blocs de répétition pour éliminer le bruit $w \oplus w'$ (tolérant jusqu'à 20% d'inversions de bits).
   - Extraction de la clé candidate $R'$.
   - Validation stricte de l'engagement :
     $$\text{Si } \text{SHA-256}(R' \parallel \text{Salt}) == C \implies \text{Succès et dérivation de } K_{\text{master}}, \text{ sinon Rejet}.$$

---

## Modèle de Menace (STRIDE & ISO/IEC)

| Catégorie STRIDE | Vecteur d'Attaque Spécifique | Mesure de Défense & Invariant Hakille |
| :--- | :--- | :--- |
| **Spoofing** (Usurpation) | Injection d'un ciphertext fabriqué ou d'une clé compromise. | Tag Poly1305 128 bits calculé sur AAD non falsifiable ; rejet instantané en cas d'altération d'un seul bit. |
| **Tampering** (Altération) | Permutation de blocs chiffrés entre la table d'identité et la table de mémoire (*Splicing*). | L'AAD lie explicitement `table_id`, `tenant_id`, et `record_uuid`. Une greffe de ciphertext échoue immédiatement à la vérification MAC. |
| **Repudiation** (Répudiation) | Répudiation de modification ou de révocation de clé de récupération. | Horodatage cryptographique signé et checksum SHA-256 pour chaque part de Shamir émise. |
| **Information Disclosure** (Fuite d'info) | Analyse médico-légale du fichier `redb` après vol physique du matériel. | Tous les champs sensibles sont chiffrés sous XChaCha20-Poly1305. Les index de recherche sont des HMAC-SHA256 (blind index) irréversibles. Les clés en RAM utilisent `ZeroizeOnDrop`. |
| **Denial of Service** (Déni de service) | Soumission de parts de Shamir corrompues pour bloquer la reconstruction de la clé. | Vérification préalable de `checksum` sur chaque part avant toute opération dans $GF(2^8)$ ; exclusion des parts invalides. |
| **Elevation of Privilege** (Élévation de privilèges) | Réutilisation d'un blob chiffré dans le tenant Famille vers un tenant Entreprise. | L'isolation par tenant est inscrite dans l'AAD (`hakille:aad:v1:<tenant_id>`). Tout déchiffrement croisé produit une erreur `CryptoError::AadMismatch`. |

---

## Contrats d'Interface Rust

### Dérivation et Gestion des Clés (`hakille-core::crypto`)

```rust
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct MasterKey {
    pub key: [u8; 32],
}

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SubkeyHierarchy {
    pub identity_key: [u8; 32],
    pub memory_key: [u8; 32],
    pub credentials_key: [u8; 32],
    pub biometrics_key: [u8; 32],
    pub blind_index_key: [u8; 32],
}

pub struct MasterKeyKdf;

impl MasterKeyKdf {
    pub fn derive_master_key(passphrase: &[u8], salt: &[u8; 32], params: &Argon2Params) -> Result<MasterKey, CryptoError>;
    pub fn derive_subkeys(master: &MasterKey) -> SubkeyHierarchy;
}
```

### Chiffrement Authentifié et AAD Binding

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct XChaChaBlob {
    pub nonce: [u8; 24],
    pub ciphertext: Vec<u8>,
    pub tag: [u8; 16],
    pub aad_fingerprint: [u8; 32],
}

pub struct AadBinding<'a> {
    pub tenant_id: &'a str,
    pub user_id: &'a str,
    pub table_id: &'a str,
    pub record_uuid: &'a str,
    pub schema_version: u32,
}

impl<'a> AadBinding<'a> {
    pub fn to_bytes(&self) -> Vec<u8>;
    pub fn fingerprint(&self) -> [u8; 32];
}

pub struct XChaChaCipher;

impl XChaChaCipher {
    pub fn encrypt(plaintext: &[u8], key: &[u8; 32], aad_binding: &AadBinding) -> Result<XChaChaBlob, CryptoError>;
    pub fn decrypt(blob: &XChaChaBlob, key: &[u8; 32], aad_binding: &AadBinding) -> Result<Vec<u8>, CryptoError>;
}
```

### Indexation Aveugle

```rust
pub fn normalize_term(term: &str) -> String;
pub fn compute_blind_index(term: &str, blind_key: &[u8; 32]) -> [u8; 32];
pub fn compute_blind_prefix_index(term: &str, prefix_len: usize, blind_key: &[u8; 32]) -> [u8; 32];

pub struct BlindIndexStore {
    pub fn index_record(&mut self, record_id: &str, term: &str, blind_key: &[u8; 32]);
    pub fn find_exact(&self, term: &str, blind_key: &[u8; 32]) -> Vec<String>;
    pub fn find_by_prefix(&self, prefix: &str, prefix_len: usize, blind_key: &[u8; 32]) -> Vec<String>;
}
```

### Récupération Sociale de Shamir

```rust
#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct SecretShare {
    pub index: u8,
    pub data: [u8; 32],
    pub checksum: [u8; 4],
}

pub fn split_secret(secret: &[u8; 32], threshold: u8, total: u8) -> Result<Vec<SecretShare>, CryptoError>;
pub fn reconstruct_secret(shares: &[SecretShare]) -> Result<[u8; 32], CryptoError>;
```

### Protocole Digital Twilight et Crémation Cryptographique (*Crypto-Shredding*)

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AccountStatus {
    Active,
    Suspended,
    DigitalTwilightGrace,
    Shredded,
}

pub struct ZkIdentityStorage {
    /// Crémation cryptographique instantanée : destruction de la clé maîtresse
    /// rendant tous les enregistrements du disque irrémédiablement indéchiffrables.
    pub fn execute_crypto_shredding(&mut self, user_id: &UserId) -> Result<(), HakilleError>;
}
```

---

## Conséquences et Garanties de Conformité

### Positives
1. **Confidentialité Mathématiquement Prouvée** : Aucun texte en clair n'atteint jamais le sous-système d'E/S disque.
2. **Recherche O(1) sans Déchiffrement Massif** : L'index aveugle préserve la vélocité native de `redb` sans fuite de mots-clés.
3. **Éradication du Risque de Splicing** : La signature AAD bloque toute manipulation frauduleuse de blocs chiffrés.
4. **Conformité RGPD Art. 17 Instantanée** : La destruction de la clé ($K_{\text{id}}$) équivaut à un effacement parfait en $O(1)$, même sur support Flash SSD sans écrasement physique.
5. **Résistance Post-Compromission** : Grâce à `ZeroizeOnDrop`, les clés en mémoire vive sont effacées dès la libération de leur portée lexicale.

### Négatives et Compromis
1. **Indexation Floue Limitée** : Les recherches approchées complexes (distance de Levenshtein) ne peuvent s'effectuer directement sur disque et nécessitent soit un index d'artéfacts de $n$-grammes aveugles, soit un filtrage en mémoire vive après extraction préliminaire.
2. **Coût Mémoire de Dérivation** : L'exécution d'Argon2id avec 64 MiB de mémoire requiert ~80 ms sur CPU moderne lors de l'ouverture initiale de session.
