# Hakille Sovereign Master Plan : Architecture Globale d'Authentification & d'Identité Souveraine

> **Document de Référence Majeur** — Système d'Exploitation Cognitif Hakille  
> **Auteur** : Équipe d'Architecture Système & Cryptographie Souveraine  
> **Statut** : Approuvé & En Cours d'Implémentation Active (Milestones M1–M6)  
> **Environnement Cible** : Pure Rust (MSRV 1.95, Rust Edition 2024, Zéro Runtime Python, Zero-Cloud Dependency)  
> **Dépôt** : `crates/hakille-core`, `crates/hakille-identity`, `crates/hakille-auth`, `crates/hakille-keystore`, `crates/hakille-perception`, `crates/hakille-ui-shell`

---

## 1. Manifeste & Vision : Le Paradigme de l'Identité Souveraine pour OS Cognitif

### 1.1 La Faillite des Modèles d'Identité Conventionnels
Depuis l'avènement de l'informatique connectée, l'authentification des utilisateurs est demeurée prisonnière d'une contradiction fondamentale : confier la vérification de l'intimité humaine à des tiers de confiance centralisés (Fournisseurs d'identité OAuth2/OIDC, coffres-forts cloud, gestionnaires de Passkeys synchronisés sur des serveurs distants, autorités de certification propriétaires).

Cette centralisation produit des vulnérabilités inacceptables :
1. **Surveillance de masse et profilage comportemental** : Chaque connexion à un service expose les habitudes de vie et le graphe social de l'utilisateur à des conglomérats publicitaires.
2. **Point unique de défaillance et de censure** : La suspension arbitraire d'un compte centralisé anéantit l'accès à l'ensemble des données numériques de l'individu.
3. **Anachronisme de l'authentification ponctuelle** : Saisir un mot de passe ou appuyer sur un capteur d'empreinte digitale au démarrage d'une session suppose que l'utilisateur demeure seul et légitime pendant des heures. En réalité, les contextes de travail et de vie partagée (famille, bureau, mobilité) exigent une adaptation continue.
4. **Inadéquation radicale face aux agents autonomes** : Les agents logiciels dotés de capacités d'action cognitive ne peuvent pas être sécurisés par de simples jetons d'accès statiques. Sans limitation mathématique de leur autorité, un sous-agent corrompu peut siphonner des comptes ou compromettre la vie privée de son propriétaire.

### 1.2 Le Saut Épistémologique de Hakille
Hakille n'est pas une simple application ni un chatbot : c'est un **Système d'Exploitation Cognitif Souverain Local-First**. L'identité dans Hakille n'est pas une permission accordée par un serveur distant, mais une **réalité cryptographique auto-émise, auto-vérifiée et physiquement ancrée dans l'appareil de l'utilisateur**.

```
+----------------------------------------------------------------------------------------------------+
|                                 LES 7 PILIERS DE LA SOUVERAINETÉ HAKILLE                           |
|                                                                                                    |
|  1. LOCAL-FIRST ABSOLU         : Zéro appel réseau requis pour s'enrôler, s'authentifier ou agir.  |
|  2. ZÉRO-PYTHON EN RUNTIME     : Déterminisme mémoire total, surface d'attaque réduite à Rust pur. |
|  3. CRYPTOGRAPHIE QUANTIQUE    : Anticipation active de la transition PQC (NIST FIPS 203/204).     |
|  4. CONFIDENTIALITÉ PROUVÉE    : Stockage ZK sur disque, index aveugles, crémation en O(1).        |
|  5. BIOMÉTRIE AUTO-RÉVOCABLE   : Biohashing annulable, zéro image brute persistée, RAM zeroized.   |
|  6. MULTI-TENANCY CONSTITUTION : Règle 7 NoWallet inviolable, airgap strict Famille vs Entreprise. |
|  7. CONFINEMENT AGENTIQUE      : Jetons Biscuit v5 Datalog, atténuation monotone bornée D <= 4.    |
+----------------------------------------------------------------------------------------------------+
```

---

## 2. Feuille de Route de Transition Cryptographique Post-Quantique (PQC)

### 2.1 La Menace "Harvest Now, Decrypt Later"
Les données mémorielles et d'identité persistées aujourd'hui par les utilisateurs de Hakille sont vouées à une longévité de plusieurs décennies. L'émergence prévisible d'ordinateurs quantiques à tolérance de pannes (FTQC) exécutant l'algorithme de Shor menace d'anéantir la sécurité des schémas asymétriques classiques basés sur les courbes elliptiques (Ed25519, X25519, ECDSA) et RSA.

Pour prémunir le système contre la capture préventive de flux chiffrés (*Harvest Now, Decrypt Later*), Hakille déploie une stratégie de chiffrement et de signature **hybride classique + post-quantique**.

```
+----------------------------------------------------------------------------------------------------+
|                                  ENCAPSULATION HYBRIDE CLASSIQUE / PQC                             |
|                                                                                                    |
|   Alice (Émetteur)                                                   Bob (Destinataire)            |
|   +-----------------------+                                          +-----------------------+     |
|   | Clé Publique X25519   |                                          | Clé Privée X25519     |     |
|   | Clé Publique ML-KEM768|                                          | Clé Privée ML-KEM768  |     |
|   +-----------------------+                                          +-----------------------+     |
|               |                                                                  |                 |
|               +-----------------------> [ Canal Infiltrable ] -------------------+                 |
|                                         Ciphertext Hybride                                         |
|                                         (C_X25519 || C_ML-KEM)                                     |
|               |                                                                  |                 |
|               v                                                                  v                 |
|   Secret Partagé SS_classic                                          Secret Partagé SS_classic     |
|   Secret Partagé SS_pqc                                              Secret Partagé SS_pqc         |
|               \                                                                  /                 |
|                +----------------------------------------------------------------+                  |
|                                                 |                                                  |
|                                                 v                                                  |
|                      K_final = HKDF-Extract(SS_classic || SS_pqc, salt)                            |
|                                                 |                                                  |
|                                                 v                                                  |
|                                      XChaCha20-Poly1305 AEAD                                       |
+----------------------------------------------------------------------------------------------------+
```

### 2.2 Standards NIST Retenus et Alignement Rust

| Primitive Cryptographique | Standard Post-Quantique | Implémentation Rust Pure | Rôle dans Hakille |
| :--- | :--- | :--- | :--- |
| **KEM (Key Encapsulation)** | **ML-KEM (Kyber-768 / FIPS 203)** | `pqcrypto-kyber` / Rust natif | Dérivation hybride de sessions de synchronisation CRDT et échange de clés inter-dispositifs. |
| **Signature Numérique** | **ML-DSA (Dilithium-3 / FIPS 204)** | `pqcrypto-dilithium` | Signatures de racines d'identité pour organisations critiques et conglomérats d'entreprises. |
| **Signature Stateful Hash** | **SLH-DSA (SPHINCS+ / FIPS 205)** | `pqcrypto-sphincsplus` | Signature immuable des règles constitutionnelles et des clés de réémission de trousseaux. |
| **Chiffrement Symétrique** | **XChaCha20-Poly1305** | `chacha20poly1305` (RustCrypto) | Clé de 256 bits offrant une marge de sécurité de 128 bits face à l'algorithme de Grover quantique. |

### 2.3 Formule de Combinaison Hybride Déterministe
Pour tout échange de clés ou dérivation de trousseau multi-périphériques, la clé symétrique opérationnelle $K_{\text{session}}$ est produite par concaténation non commutative dans HKDF :

$$K_{\text{session}} = \text{HKDF-Expand}\left(\text{HKDF-Extract}(S_{\text{salt}}, \, \text{ECDH}_{\text{X25519}}(sk_A, pk_B) \parallel \text{Decaps}_{\text{ML-KEM}}(ct_{\text{pqc}}, sk_B)), \, \text{"hakille:hybrid-pqc:v1"}, \, 32\right)$$

Cette formulation garantit que même en cas de rupture mathématique totale de l'un des deux systèmes (classique ou quantique), la confidentialité du canal demeure inviolable.

---

## 3. Identité Décentralisée Auto-Souveraine (SSI), DIDs et W3C Verifiable Credentials

### 3.1 Méthodes de DID Intégrées
Hakille rejette toute dépendance vis-à-vis d'une blockchain publique coûteuse, lente et destructrice d'anonymat pour les transactions d'identité quotidiennes. Le système privilégie les identifiants décentralisés cryptographiquement auto-résolus :

```
                                  +-----------------------+
                                  | IDENTIFIANTS SOUVERAINS|
                                  +-----------------------+
                                              |
                +-----------------------------+-----------------------------+
                |                                                           |
                v                                                           v
   +-------------------------+                                 +-------------------------+
   |        did:key          |                                 |         did:jwk         |
   |                         |                                 |                         |
   | did:key:z6Mku...        |                                 | did:jwk:eyJrdHkiOi...   |
   | Ed25519 (W3C standard)  |                                 | Export pour fédération  |
   | Résolution O(1) locale  |                                 | web d'entreprise et SCIM|
   +-------------------------+                                 +-------------------------+
```

1. **`did:key` (Format Principal)** : L'identifiant est la représentation multicodec de la clé publique elle-même (Ed25519 pour les agents et utilisateurs, Secp256k1 pour les passerelles financières, ou Kyber/Dilithium pour les identités post-quantiques). Résolution instantanée en $O(1)$ sans requête réseau.
2. **`did:jwk`** : Utilisé pour l'interopérabilité directe avec les protocoles d'entreprise modernes (OpenID for Verifiable Credentials, SCIM 2.0).

### 3.2 Justificatifs Vérifiables W3C (Verifiable Credentials v2.0)
Les attributs d'identité (habilitations professionnelles, diplômes, autorisations parentales, certificats d'audit de sécurité) sont matérialisés sous forme de Verifiable Credentials conformes à la recommandation W3C VC 2.0 :

```json
{
  "@context": [
    "https://www.w3.org/ns/credentials/v2",
    "https://schema.hakille.org/identity/v1"
  ],
  "id": "urn:uuid:f81d4fae-7dec-11d0-a765-00a0c91e6bf6",
  "type": ["VerifiableCredential", "HakilleAgentPassport"],
  "issuer": "did:key:z6MkuPrincipalAuthor",
  "validFrom": "2026-11-06T18:00:00Z",
  "validUntil": "2026-11-06T19:00:00Z",
  "credentialSubject": {
    "id": "did:key:z6MksAgentWorker",
    "delegationDepth": 1,
    "maxFuel": 1000000,
    "budgetLimitCents": 500,
    "constitutionalHash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
  },
  "proof": {
    "type": "Ed25519Signature2020",
    "verificationMethod": "did:key:z6MkuPrincipalAuthor#z6MkuPrincipalAuthor",
    "proofValue": "z3h9j..."
  }
}
```

### 3.3 Divulgation Sélective ZK via BBS+ et Conformité eIDAS 2.0
Pour prouver des assertions d'identité sans révéler de données personnelles (par exemple : prouver qu'un utilisateur est majeur sans dévoiler sa date de naissance, ou prouver son appartenance à une organisation sans révéler son matricule d'employé), Hakille intègre les signatures multi-messages BBS+.
L'utilisateur génère une preuve Zero-Knowledge de possession de justificatif :

$$\pi_{\text{ZKP}} = \text{BBS+CreateProof}(\text{Credential}, \text{DisclosedAttributes}, \text{Nonce})$$

Le vérificateur valide mathématiquement la preuve contre la clé publique de l'émetteur sans jamais avoir accès aux attributs masqués. Cette architecture garantit une compatibilité native anticipée avec l'architecture EUDI Wallet d'eIDAS 2.0.

---

## 4. Gouvernance et Confinement des Essaims d'Agents Autonomes

### 4.1 La Théorie des Capacités Décentralisées Biscuit v5
Hakille bannit définitivement le modèle ACL / RBAC centralisé pour l'interaction entre agents cognitifs. Les autorisations d'action sont portées par des **jetons de capacité Biscuit v5** utilisant le langage logique Datalog.

```
+----------------------------------------------------------------------------------------------------+
|                         STRUCTURE D'UN JETON DE CAPACITÉ BISCUIT V5 D'AGENT                        |
|                                                                                                    |
|   +--------------------------------------------------------------------------------------------+   |
|   | BLOC RACINE 0 (Signé par la Racine Hakille ou l'Utilisateur Principal)                     |   |
|   |  * Faits : right("filesystem", "read"), right("filesystem", "write"), right("network")    |   |
|   |  * Règle : valid_user($u) <- user($u), active($u)                                          |   |
|   |  * Caveat: check if operation_time() <= 1730920000                                         |   |
|   +--------------------------------------------------------------------------------------------+   |
|                                                  |                                                 |
|                                                  v Atténuation Monotone Hors-Ligne (D = 1)         |
|   +--------------------------------------------------------------------------------------------+   |
|   | BLOC 1 (Ajouté par l'Agent 1 sans contacter l'autorité)                                   |   |
|   |  * Caveat: check if resource_path($p), $p.starts_with("/home/antigravity/workspace")       |   |
|   |  * Caveat: check if execution_budget($b), $b <= 200                                        |   |
|   +--------------------------------------------------------------------------------------------+   |
|                                                  |                                                 |
|                                                  v Atténuation Monotone Hors-Ligne (D = 2)         |
|   +--------------------------------------------------------------------------------------------+   |
|   | BLOC 2 (Ajouté par l'Agent 2 pour un sous-agent de lecture)                                |   |
|   |  * Caveat: check if right("filesystem", "read")  // Droit "write" et "network" éliminés   |   |
|   |  * Caveat: check if execution_fuel($f), $f <= 50000                                        |   |
|   +--------------------------------------------------------------------------------------------+   |
+----------------------------------------------------------------------------------------------------+
```

### 4.2 Invariant Hard de Délégation Bornée ($D_{\text{max}} = 4$)
Pour éliminer les risques d'essaims récursifs auto-proliférants et de cascades de mandataires confus (*Confused Deputy Cascade*), Hakille applique une règle matérielle immuable :

$$D_{\text{agent}} \le D_{\text{max}} = 4$$

Tout agent tentant de créer un descendant à la profondeur $D = 5$ voit son appel système rejeté unilatéralement avec l'erreur `HakilleError::CapabilityDenied("chaîne de délégation trop profonde")`.

### 4.3 Les 4 Règles Constitutionnelles de l'Identité Agentique

1. **Règle 1 (Identité Dérivée Non Élevable)** : Un agent ne peut jamais posséder des privilèges supérieurs à ceux de son créateur direct ($\text{Priv}(A_{k+1}) \subsetneq \text{Priv}(A_k)$).
2. **Règle 2 (Traçabilité Cryptographique du Journal)** : Toute action entreprise par un agent injecte son DID dans le journal append-only `hakille-audit`, signé par la clé éphémère de son passeport.
3. **Règle 4 (Confinement Temporel et Budgétaire Strict)** : Tout passeport d'agent possède une durée de vie finie en secondes ($\le 3600\,\text{s}$ par défaut) et un budget financier non rechargeable sans approbation explicite de l'utilisateur.
4. **Règle 7 (Cloisonnement des Mineurs & NoWallet)** : Aucun agent agissant pour le compte d'un profil mineur ne peut se voir déléguer la capacité `CapabilityScope::Wallet`.

---

## 5. Perception Biométrique Ambiante & Pipeline Vision 100% Rust

### 5.1 Architecture Neuronale Embarquée

Le pipeline d'authentification faciale de Hakille fonctionne sans aucune dépendance Python ou service distant :

```
+----------------------------------------------------------------------------------------------------+
|                                 PIPELINE VISION HAKILLE PURE RUST                                  |
|                                                                                                    |
|    Capture Flux Caméra RGB (RAM Ephemeral, ZeroizeOnDrop)                                          |
|                           |                                                                        |
|                           v                                                                        |
|    +----------------------------------------------------+                                          |
|    | InsightFace SCRFD-0.5GF (~1.5 Mo, Latence 3.2 ms)  |                                          |
|    | Détection BoundingBox (x,y,w,h) + 5 Points Repères |                                          |
|    +----------------------------------------------------+                                          |
|                           |                                                                        |
|                           v                                                                        |
|    +----------------------------------------------------+                                          |
|    | Alignement Affine 2D & Recadrage Normalisé 112x112 |                                          |
|    +----------------------------------------------------+                                          |
|                           |                                                                        |
|              +------------+------------+                                                           |
|              |                         |                                                           |
|              v                         v                                                           |
|   +---------------------+   +---------------------------------------+                              |
|   | MiniFASNetV2        |   | Défi Actif Aléatoire (Validité 10s)   |                              |
|   | Anti-Spoofing PAD   |   | EAR Blinks, Regard, solvePnP 3D Pose |                              |
|   +---------------------+   +---------------------------------------+                              |
|              \                         /                                                           |
|               +-----------+-----------+                                                            |
|                           | (Vérification Vivant Validée)                                          |
|                           v                                                                        |
|    +----------------------------------------------------+                                          |
|    | Idiap EdgeFace-XS / MobileFaceNet (99.73% LFW)     |                                          |
|    | Extraction d'Embedding Unitaire 512D L2-normalisé  |                                          |
|    +----------------------------------------------------+                                          |
|                           |                                                                        |
|              +------------+------------+                                                           |
|              |                         |                                                           |
|              v                         v                                                           |
|   +---------------------+   +---------------------------------------+                              |
|   | Biohashing          |   | Fuzzy Extractor (Code-Offset BCH)     |                              |
|   | Projection Salée    |   | Helper Data P = w XOR C_R             |                              |
|   | 1024 bits Annulables|   | Engagement C = SHA-256(R || Salt)     |                              |
|   +---------------------+   +---------------------------------------+                              |
|                           |                                                                        |
|                           v                                                                        |
|    Purge Immédiate de la RAM : ZeroizeOnDrop sur les Pixels & Flottants                            |
+----------------------------------------------------------------------------------------------------+
```

### 5.2 Formulations Mathématiques du Pipeline

#### Distance Cosinus et Seuil de Décision
Soit $\mathbf{u}, \mathbf{v} \in \mathbb{R}^{512}$ deux embeddings faciaux normalisés ($\|\mathbf{u}\|_2 = \|\mathbf{v}\|_2 = 1.0$) :

$$d_{\text{cos}}(\mathbf{u}, \mathbf{v}) = 1 - \langle \mathbf{u}, \mathbf{v} \rangle = 1 - \sum_{k=1}^{512} u_k \cdot v_k$$

- **Seuil de similarité d'acceptation** : $d_{\text{cos}} \le 0.40 \iff \text{Similarité Cosinus } \ge 0.60$.
- Taux de fausse acceptation théorique $\text{FAR} < 10^{-5}$ et précision de 99.73% sur le standard international LFW.

#### Ratio d'Aspect Oculaire (Eye Aspect Ratio - EAR)
Pour les 6 points du contour oculaire $[p_1, \dots, p_6]$ :

$$\text{EAR} = \frac{\|p_2 - p_6\|_2 + \|p_3 - p_5\|_2}{2 \|p_1 - p_4\|_2}$$

Un clignement valide requiert $\text{EAR} < 0.20$ suivi d'une réouverture à $\text{EAR} \ge 0.28$.

#### Estimation de Pose Faciale 3D par solvePnP
La matrice de rotation $R \in SO(3)$ et le vecteur de translation $\mathbf{T} \in \mathbb{R}^3$ sont calculés via l'équation de projection sténopé :

$$s \begin{bmatrix} u \\ v \\ 1 \end{bmatrix} = \begin{bmatrix} f_x & 0 & c_x \\ 0 & f_y & c_y \\ 0 & 0 & 1 \end{bmatrix} \begin{bmatrix} R & \mathbf{T} \end{bmatrix} \begin{bmatrix} X_w \\ Y_w \\ Z_w \\ 1 \end{bmatrix}$$

Le défi actif impose à l'utilisateur d'exécuter une déviation angulaire de lacet (*Yaw*) ou de tangage (*Pitch*) choisie stochastiquement (e.g. $\text{Yaw} = +25^\circ \pm 5^\circ$) dans une fenêtre de validité de 10 secondes.

---

## 6. Moteur de Fusion Multimodale Continue & Décroissance Temporelle

### 6.1 Dynamique de la Confiance Continue ($\tau$)
Contrairement à un verrou traditionnel statique (ouvert ou fermé), la session Hakille est régie par une variable d'état continue $\tau(t) \in [0.0, 1.0]$.
Trois signaux ambiants faibles sont fusionnés en temps réel avec leurs poids respectifs :

$$\tau_{\text{inst}}(t) = \frac{\omega_{\text{face}} \cdot s_{\text{face}}(t) + \omega_{\text{voice}} \cdot s_{\text{voice}}(t) + \omega_{\text{prox}} \cdot s_{\text{prox}}(t)}{\omega_{\text{face}} + \omega_{\text{voice}} + \omega_{\text{prox}}}$$

avec les poids de calibration :
- $\omega_{\text{face}} = 0.50$ (Reconnaissance faciale continue par caméra discrète)
- $\omega_{\text{voice}} = 0.35$ (Empreinte vocale extraite lors des interactions vocales ambiantes)
- $\omega_{\text{prox}} = 0.15$ (Proximité de balise personnelle BLE/UWB)

### 6.2 Décroissance Temporelle Exponentielle
En l'absence de nouvelle observation d'un signal, la confiance décroît de manière continue selon la loi :

$$\tau(t + \Delta t) = \tau(t) \cdot 2^{-\frac{\Delta t}{t_{1/2}}}$$

où la demi-vie $t_{1/2} = 180\,\text{secondes}$ (3 minutes).

```
Plages de Confiance tau   Capacités Opérationnelles Autorisées
+-----------------------+----------------------------------------------------------------------------+
| 0.90 <= tau <= 1.00   | ACCÈS COMPLET SOUVERAIN (Wallet, Signature Contrats, Export Trousseau)      |
| 0.80 <= tau <  0.90   | OPÉRATIONS STANDARD COGNITIVES (Lecture/Écriture Notes, Agents Swarm, Code) |
| 0.60 <= tau <  0.80   | MODE RESTREINT LECTURE SEULE (Consultation simple, Dashboard non-critique) |
| 0.00 <= tau <  0.60   | VERROUILLAGE SÉCURISÉ (Écran de veille, Défi liveness requis)              |
+-----------------------+----------------------------------------------------------------------------+
```

- **Fail-Closed Ultime** : À $\Delta t \ge 900\,\text{s}$ (15 minutes consécutives sans détection), la session est révoquée de plein droit et les sous-clés en RAM sont écrasées par zéros.

---

## 7. Hiérarchie Multi-Tenant à 5 Niveaux Cardinaux & Pare-Feu Inter-Tenants

### 7.1 Typologie des 5 Niveaux Cardinaux

```
+----------------------------------------------------------------------------------------------------+
|                                    LES 5 TIERS MULTI-TENANTS HAKILLE                               |
|                                                                                                    |
|  1. TEAMS (Équipes)          : Espaces de collaboration légers pour humains et essaims d'agents.   |
|  2. ORGANIZATIONS (Orgs)     : Entités juridiques formelles, synchronisation SCIM, officiers RGPD. |
|  3. ENTERPRISES (Entreprises): Conglomérats multi-organisations, racine HSM/KMS matérielle.       |
|  4. FAMILIES (Familles)      : Sphère intime domestique, protection mineurs Règle 7, invités 24h.   |
|  5. GROUPS (Groupes)         : Cercles ad-hoc transversaux, adhésion par quorum cryptographique.   |
+----------------------------------------------------------------------------------------------------+
```

### 7.2 L'Invariant Inviolable de la Sphère Familiale (Règle 7)
Le tenant de type `Family` possède des garanties constitutionnelles absolues :
1. **Protection Mineurs `NoWallet`** : Tout membre associé à un `MinorProfile` (Enfants, Adolescents) voit l'émission de capacités financières (`CapabilityScope::Wallet`) et de modification de configuration système rejetée directement dans le système de types Rust.
2. **Bornage des Invités (`GuestSession`)** : Les profils invités temporaires créés sur le matériel du foyer sont bridés à une validité maximale de 24 heures ($t_{\text{exp}} \le 1440\,\text{minutes}$) et ont interdiction formelle d'accéder au portefeuille ou au noyau complet.
3. **Airgap Hermétique Famille vs Entreprise** : Le `CrossTenantFirewall` interdit tout partage ou pontage automatique entre un tenant `Family` et un tenant `Organization`/`Enterprise`, protégeant la vie privée domestique contre les exigences de surveillance corporative.

---

## 8. Moteur d'Accès Hybride : ReBAC Zanzibar + RBAC + ABAC + Biscuit

### 8.1 Algèbre Relationnelle Zanzibar
Les permissions au sein des tenants s'expriment sous forme de tuples relationnels :

$$\langle \text{entity\_type} \rangle : \langle \text{entity\_id} \rangle \# \langle \text{relation} \rangle @ \langle \text{subject\_type} \rangle : \langle \text{subject\_id} \rangle$$

Exemples concrets de tuples Hakille :
- `organization:acme_corp#compliance_officer@user:charlotte`
- `team:research_lab#member@agent:did:key:z6Mks4...`
- `family:home_vault#guardian@user:alice`
- `family:home_vault#dependent@user:bob_minor`

### 8.2 Matrice d'Évaluation Hybride et Détection de Boucles
L'évaluation s'exécute selon un algorithme ordonné en cascade garantissant une latence sub-milliseconde :

```
Requête (Subject, Action, Resource, Context, BiscuitToken)
  |
  +---> 1. Pare-feu Inter-Tenants (Vérification d'isolation hermétique)
  |
  +---> 2. Traversée DFS ReBAC Zanzibar (Résolution transitive + détection de cycle O(1))
  |
  +---> 3. Validation RBAC Statique (Rôle socle Owner/Admin/Member)
  |
  +---> 4. Validation ABAC Contextuelle (Niveau matériel Enclave Tier 1-3 et score tau)
  |
  +---> 5. Exécution Datalog du Jeton Biscuit (Vérification des caveats d'atténuation)
  |
  v
DÉCISION : ALLOW ou DENY(Explication Raisonnée)
```

---

## 9. Stockage Zero-Knowledge, Indexation Aveugle & Crémation Cryptographique

### 9.1 Dérivation et Isolement des Clés (Argon2id + HKDF)
À partir de la passphrase maîtresse, le système applique Argon2id (RFC 9106) avec un profil de production résistant aux attaques matérielles :

$$K_{\text{master}} = \text{Argon2id}(P, S, m=65536\,\text{KiB}, t=3, p=4, L=32)$$

L'arborescence HKDF-SHA256 isole ensuite 5 sous-clés indépendantes :
- $K_{\text{id}}$ : Clé des enregistrements de profil d'identité
- $K_{\text{mem}}$ : Clé des souvenirs épisodiques et de mémoire cognitive
- $K_{\text{cred}}$ : Clé des justificatifs et des jetons d'authentification
- $K_{\text{bio}}$ : Clé de projection pour le Biohashing
- $K_{\text{bindex}}$ : Clé secrète des index aveugles

### 9.2 Chiffrement Symétrique Authentifié XChaCha20-Poly1305 avec AAD Binding
Chaque enregistrement sur disque est encapsulé dans un `XChaChaBlob` :
- Nonce étendu de 192 bits (24 octets) généré aléatoirement
- Tag d'authentification Poly1305 de 128 bits (16 octets)
- Données Authentifiées Additionnelles (AAD) liant cryptographiquement l'identifiant du tenant, l'utilisateur, la table et l'UUID de l'enregistrement :

$$\text{AAD} = \text{"hakille:aad:v1:"} \parallel \text{tenant\_id} \parallel \mathtt{0x1F} \parallel \text{user\_id} \parallel \mathtt{0x1F} \parallel \text{table\_id} \parallel \mathtt{0x1F} \parallel \text{record\_uuid} \parallel \mathtt{0x1F} \parallel \text{schema\_version}_4$$

Cette liaison rend mathématiquement impossible l'interversion de blocs chiffrés entre tables ou tenants distincts (*Anti-Splicing Invariant*).

### 9.3 Indexation Aveugle Exacte et par Préfixe (Blind Indexing)
Les recherches textuelles rapides s'effectuent sans jamais déchiffrer la base de données :
- **Index Exact** :
  $$T_{\text{exact}} = \text{HMAC-SHA256}(K_{\text{bindex}}, \text{"hakille:blind:exact:v1:"} \parallel \text{Normalize}(w))$$
- **Index par Préfixe** (longueur $\ell \in [3, 8]$) :
  $$T_{\text{prefix}} = \text{HMAC-SHA256}(K_{\text{bindex}}, \text{"hakille:blind:prefix:v1:"} \parallel \ell \parallel \text{":"} \parallel \text{Prefix}(w, \ell))$$

### 9.4 Récupération Sociale de Shamir dans $GF(2^8)$ et Crémation (*Crypto-Shredding*)
- **Partage Social $k$-parmi-$n$** : La clé de secours est découpée en parts polynomiales dans $GF(2^8)$ avec le polynôme irréductible $P(x) = x^8 + x^4 + x^3 + x + 1$ (`0x11b`). La reconstruction exige le quorum de $k$ fiduciaires de confiance.
- **Crémation Cryptographique Instantanée (Digital Twilight)** : En vertu du droit à l'effacement (RGPD Art. 17), l'utilisateur peut déclencher la destruction de $K_{\text{id}}$. En quelques microsecondes, l'écrasement en mémoire de cette sous-clé rend des téraoctets de données sur disque définitivement et irréversiblement indéchiffrables sans usure matérielle du SSD.

---

## 10. Intégration Native dans le Shell UI (Slint / Native Winit & Wgpu)

### 10.1 Affichage Tête Haute (HUD) et Jauge de Confiance Ambiante
L'interface utilisateur de Hakille intègre nativement l'état d'authentification souveraine sans encombrer l'espace de travail cognitif :

```
+----------------------------------------------------------------------------------------------------+
|  [HAKILLE SHELL TOP BAR]                                                                           |
|                                                                                                    |
|  [Icone Système]  Workspace: AI Research   Tenant: ACME Lab (Team)         [HUD AUTH SOUVERAINE]   |
|                                                                                                    |
|                                                     +------------------------------------------+   |
|                                                     | [V] Alice  | tau: 0.94 [======] | Tier 1 |   |
|                                                     +------------------------------------------+   |
+----------------------------------------------------------------------------------------------------+
```

1. **Jauge Dynamique $\tau$** : Indicateur de confiance en temps réel avec changement de couleur subtil (Vert émeraude pour $\tau \ge 0.90$, Ambre doré pour $0.80 \le \tau < 0.90$, Orange crépusculaire pour $0.60 \le \tau < 0.80$, Rouge pourpre clignotant avant verrouillage).
2. **Indicateur Matériel de Sécurité** : Badge affichant le niveau d'ancrage de la session :
   - `Tier 1` : TPM 2.0 / Apple Secure Enclave actif
   - `Tier 2` : Hôte logiciel sécurisé avec mémoire protégée
   - `Tier 3` : Environnement bac à sable non attesté

### 10.2 Modale d'Élévation de Privilèges (*Step-Up Auth*)
Lorsqu'une action requiert $\tau \ge 0.90$ et que la confiance courante est insuffisante, une modale native Slint apparaît instantanément au centre de l'écran :
- Animation d'un cercle de visée guidant le regard de l'utilisateur
- Indication du défi actif en cours (ex : "Clignez des yeux" ou "Regardez la cible bleue")
- Rétroaction visuelle immédiate et retour fluide à l'application dès la validation du défi (latence totale $< 350\,\text{ms}$).

### 10.3 Assistant d'Enrôlement Biométrique Souverain
Un assistant guidé pas à pas permet l'enrôlement initial :
- Vérification que la caméra fonctionne en local sans miroir cloud
- Capture de 3 angles légers (face, inclinaison $+15^\circ$, $-15^\circ$)
- Calcul instantané du Biohash annulable et affichage des empreintes cryptographiques publiques
- Option de configuration du partage social de Shamir avec sélection des tiers de confiance.

---

## 11. Matrice de Menace Adversariale et Résilience Systémique

| Scénario d'Attaque Adversariale | Vecteur Technique Exploité | Contre-Mesure Native & Invariant Hakille |
| :--- | :--- | :--- |
| **Vol Physique de Machine Hôte** | Démontage de SSD et analyse médico-légale des bases de données. | Zéro-Knowledge sur disque : Chiffrement XChaCha20-Poly1305 systématique, index aveugles HMAC-SHA256, clé maître Argon2id (64 MiB). |
| **Attaque par Présentation Écran 8K** | Rejeu d'une vidéo 8K ou photo haute fidélité devant la webcam. | Détection passive MiniFASNetV2 par analyse spectrale Fourier 2D (effet de moiré sous-pixel) + défi liveness actif aléatoire. |
| **Deepfake Vidéo en Temps Réel** | Flux vidéo virtuel injecté par diffusion latente lors d'une session. | Défis actifs stochastiques à nonce éphémère (10s) : déviation d'angle 3D solvePnP et dynamique pupillaire impossible à synthétiser sans artefacts. |
| **Attaque par Découpage-Collage (*Splicing*)** | Permutation d'enregistrements chiffrés d'une table à une autre. | Liaison AAD canonique liant explicitement `tenant_id`, `table_id` et `record_uuid`. Échec immédiat de validation MAC Poly1305. |
| **Attaque du Mandataire Confus (*Confused Deputy*)** | Un sous-agent détourne l'autorité de son créateur pour vider un portefeuille. | Plafonnement strict $D_{\text{max}} = 4$, atténuation Datalog monotone des jetons Biscuit, et Règle 7 interdisant le Wallet aux mineurs et invités. |
| **Dépassement de Capacité Mémoire** | Buffer overflow pour tenter d'extraire les clés de déchiffrement en RAM. | Implémentation intégrale en **Rust pur** avec garanties strictes de sûreté mémoire et écrasement automatique `ZeroizeOnDrop`. |
| **Capture Préventive Quantique** | Enregistrement de données aujourd'hui pour déchiffrement quantique futur. | Architecture hybride combinant XChaCha20-Poly1305 (256-bit) et le standard post-quantique NIST ML-KEM-768 (Kyber). |
| **Injection de Prompt Indirecte** | Un document externe malveillant ordonne à l'agent de signer une transaction. | Le modèle d'isolation Hakille classe tout contenu externe comme `Untrusted` immuable : aucun chemin d'exécution ne permet d'élever ce tag en capability. |

---

## 12. Feuille de Route d'Implémentation & Jalons de Validation (Milestones M1–M6)

```
                                      HAKILLE ROADMAP SOUVERAINE
                                      
  [ M1 : Cœur Cryptographique & ZK Storage ] ------------------------------------+ (Terminé / Consolidé)
  * Argon2id KDF, HKDF subkeys, XChaCha20 AEAD, Blind Indexing, Shamir VSS        |
                                                                                 v
  [ M2 : Hiérarchie Multi-Tenant & ReBAC ] --------------------------------------+ (Terminé / Consolidé)
  * 5 Tiers, Zanzibar ReBAC, Biscuit v5 Datalog, Bounded Delegation (D <= 4)     |
                                                                                 v
  [ M3 : Biométrie Faciale & Anti-Usurpation ] ----------------------------------+ (Terminé / Consolidé)
  * SCRFD-0.5GF, EdgeFace-XS, MiniFASNetV2 PAD, Biohashing, RGPD & AI Act        |
                                                                                 v
  [ M4 : Fusion Multimodale & Intégration Stockage ] ----------------------------+ (Terminé / Consolidé)
  * Confiance continue tau, décroissance temporelle, Step-Up Auth, Guest & Minors|
                                                                                 v
  [ M5 : Suite ADR & Master Plan Souverain ] ------------------------------------+ (CETTE LIVRAISON)
  * ADR-0012, ADR-0013, ADR-0014 enrichis, Master Plan complet /plan             |
                                                                                 v
  [ M6 : Tests E2E Opaque-Box & Durcissement Adversarial ] ----------------------+ (Validation Finale)
  * Tiers 1-4 Test Suite, Simulation d'attaques, Certification d'intégrité       
```

### Critères de Validation Forensique Indépendante
1. **Zéro Donnée en Clair sur Disque** : Validation par balayage d'entropie binaire sur les fichiers `redb` démontrant une distribution pseudo-aléatoire uniforme ($H \ge 7.999 / 8.0$).
2. **Purge Parfaite de la RAM** : Vérification par scan de mémoire vive après destruction d'objets (`ZeroizeOnDrop`) confirmant l'absence totale de résidus de clé maître ou de trames vidéo.
3. **Respect Mathématique des Invariants** : 100% de tests unitaires et d'intégration validant le rejet systématique des délégations à $D > 4$ et des accès Wallet pour mineurs et invités.
4. **Temps de Réponse Temps Réel** : Inférence faciale complète $< 25\,\text{ms}$, décision d'accès ReBAC/Biscuit $< 2\,\text{ms}$, dérivation Argon2id $< 100\,\text{ms}$.
