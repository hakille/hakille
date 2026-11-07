# ADR 014: Authentification Faciale Biométrique, Anti-Usurpation ISO/IEC 30107-3 et Fusion Multimodale Continue

## Statut
Accepté (Mis à jour pour la version souveraine de Hakille)

## Contexte
Hakille redéfinit l'expérience utilisateur d'un système d'exploitation cognitif : l'interaction vocale et visuelle permanente rend anachronique la saisie répétée de mots de passe ou de codes PIN. Cependant, l'intégration de la biométrie faciale sur un système autonome local-first affronte des contraintes extrêmes :

1. **Attaques par Présentation (*Presentation Attacks*) et Deepfakes Génératifs** : Les caméras grand public sont vulnérables aux photos imprimées haute résolution, aux écrans 4K/OLED de téléphones ou tablettes, aux masques 3D en silicone et aux flux vidéo synthétiques générés par modèles de diffusion en temps réel.
2. **Irrévocabilité des Données Biométriques** : Une image de visage ou un gabarit vectoriel brut stocké sur disque constitue une vulnérabilité à vie : contrairement à un mot de passe ou un certificat, un visage ne peut pas être réémis en cas de compromission.
3. **Absence de Runtimes Python ou de Services Cloud** : Toute l'inférence neuronale doit s'exécuter localement en **Rust pur** (Zero-Python runtime, MSRV 1.95, édition 2024), sans latence perceptible et sur du matériel grand public (CPU x86_64 et ARM64).
4. **Exigences Réglementaires Européennes (RGPD & EU AI Act)** : Le traitement de données biométriques relève de catégories particulières (RGPD Art. 9) exigeant un consentement explicite et le droit à l'effacement. De plus, le Règlement Européen sur l'IA (EU AI Act - Règlement 2024/1689) classe les systèmes biométriques en "haut risque" ou "inacceptables", imposant un confinement strict à l'exemption de vérification 1:1 d'identité individuelle sur équipement personnel.

---

## Pipeline Biométrique et Anti-Usurpation de Bout en Bout

```
+----------------------------------------------------------------------------------------------------+
|                         PIPELINE BIOMÉTRIQUE SOUVERAIN (MÉMOIRE VIVE / ZEROIZE)                     |
|                                                                                                    |
|   +--------------------------+                                                                     |
|   | Trame Caméra Vidéo RGB   | (Capture locale en RAM pure, ZeroizeOnDrop, Zéro disque)            |
|   +--------------------------+                                                                     |
|                |                                                                                   |
|                v                                                                                   |
|   [ ÉTAPE 1 : DÉTECTION FACIALE & 5 REPÈRES ]                                                      |
|   Modèle : InsightFace SCRFD-0.5GF (~1.5 Mo, Latence ~3.2ms)                                       |
|   Sortie : Boîte englobante (x, y, w, h) + 5 points [Yeux, Nez, Bords de bouche]                   |
|                |                                                                                   |
|                v                                                                                   |
|   [ ÉTAPE 2 : DÉTECTION DU VIVANT PASSIVE (PAD ISO/IEC 30107-3) ]                                  |
|   Modèle : MiniFASNetV2 (Analyse spectrale double échelle)                                         |
|   Analyse : Texture cutanée fine (Variance Laplacienne) + Filtrage spectral de Fourier 2D           |
|   Détection : Écrans 4K/8K (moiré/tramage), photos papier (reflets spéculaires), masques silicone  |
|                |                                                                                   |
|                | (Si liveness passive suspecte ou privilèges élevés)                               |
|                v                                                                                   |
|   [ ÉTAPE 3 : DÉFI-RÉPONSE ACTIF ALÉATOIRE ]                                                       |
|   Challenges : Clignements oculaires (EAR < 0.20), poursuite de regard, pose 3D solvePnP           |
|   Validité : Nonce éphémère de 10 secondes anti-rejeu                                              |
|                |                                                                                   |
|                v                                                                                   |
|   [ ÉTAPE 4 : ALIGNEMENT AFFINE & EXTRACTION D'EMBEDDING 512D ]                                    |
|   Alignement : Matrice de similarité affine 2x3 normalisant la distance inter-oculaire             |
|   Modèle : Idiap EdgeFace-XS / MobileFaceNet (99.73% LFW, 512D L2-normalisé)                       |
|                |                                                                                   |
|                v                                                                                   |
|   [ ÉTAPE 5 : GABARITS PROTÉGÉS & BIOHASHING ANNULABLE ]                                           |
|   Projection : Orthogonale aléatoire salée M_salt -> Biohash binaire 1024 bits                     |
|   Extracteur Flou (Code-Offset) : Helper data P = w XOR C_R, Engagement C = SHA-256(R || Salt)     |
|   Propriété : En cas de compromission, le sel est révoqué -> Nouveau gabarit sans changer de visage|
|                |                                                                                   |
|                v                                                                                   |
|   [ ÉTAPE 6 : PURGE INSTANTANÉE DE LA MÉMOIRE ]                                                    |
|   Les trames brutes et vecteurs flottants sont écrasés par zéros (trait ZeroizeOnDrop)            |
+----------------------------------------------------------------------------------------------------+
```

---

## Modèle de Fusion Multimodale Continue et Décroissance Temporelle

Le niveau de confiance globale de la session $\tau(t) \in [0.0, 1.0]$ évolue dynamiquement en temps réel selon les signaux perçus :

```
Confiance tau
  1.0 +---------------------------------------------------------------------------+
      |      [Défi Actif Réussi]                                                  |
  0.9 |........./''''''''''''\....................................... Seuil Wallet |
      |        /              \                                       (Admin/Paiement)
  0.8 |-------/                \---------\........................... Seuil Session
      |      /  (Présence visage)         \    (Voix seule)           Complète
  0.6 |...../                              \......................... Seuil Lecture Seule
      |    /                                \   (Absence capteur)
  0.4 |   /                                  \
      |  /                                    \
  0.2 | /                                      \
      |/                                        \
  0.0 +------------------------------------------\--------------------+-----------> Temps
      0                                           3 min (Demi-vie)   15 min (Verrouillage)
```

---

## Formulations Mathématiques et Algorithmes

### 1. Détection Faciale et Alignement Affine 2D (SCRFD-0.5GF)

La trame d'entrée subit une détection multi-échelle via SCRFD-0.5GF. Les 5 points de repère faciaux sont notés :

$$P_{\text{landmarks}} = \{ p_{\text{left\_eye}}, p_{\text{right\_eye}}, p_{\text{nose}}, p_{\text{left\_mouth}}, p_{\text{right\_mouth}} \} \subset \mathbb{R}^2$$

Soit les coordonnées de référence canoniques normalisées $P^* \subset \mathbb{R}^2$ pour un recadrage de $112 \times 112$ pixels.
La transformation affine de similarité $M = \begin{bmatrix} s \cos \theta & -s \sin \theta & t_x \\ s \sin \theta & s \cos \theta & t_y \end{bmatrix}$ est calculée par moindres carrés :

$$M^* = \arg\min_M \sum_{i=1}^5 \| M \cdot \tilde{p}_i - p^*_i \|_2^2$$

Le visage aligné $I_{\text{aligned}} = \text{WarpAffine}(I_{\text{frame}}, M^*, (112, 112))$ présente une distance inter-oculaire normalisée invariant aux rotations dans le plan.

### 2. Détection du Vivant Passive (MiniFASNetV2 & Analyse Spectrale Fourier)

L'anti-usurpation passive évalue la texture haute fréquence cutanée et les artéfacts spectraux caractéristiques des attaques par présentation (écrans et impressions) :

1. **Variance Laplacienne spatiale** :
   $$\sigma_L^2 = \frac{1}{W \cdot H} \sum_{x, y} \left( \nabla^2 I(x, y) - \mu_L \right)^2$$
   Un seuil minimal $\sigma_L^2 \ge 100.0$ élimine les images floues ou les impressions basse résolution.
2. **Rapport d'Énergie Spectrale de Fourier 2D** :
   Soit $\mathcal{F}(u, v) = \sum_{x, y} I(x, y) e^{-j 2\pi (ux/W + vy/H)}$ la transformée de Fourier 2D.
   Le spectre de puissance $S(u, v) = |\mathcal{F}(u, v)|^2$ est partitionné en basses fréquences $R_{\text{low}}$ et hautes fréquences $R_{\text{high}}$ :
   $$\rho_{\text{high}} = \frac{\iint_{R_{\text{high}}} S(u, v) \, du \, dv}{\iint_{R_{\text{total}}} S(u, v) \, du \, dv}$$
   Les écrans numériques affichent des pics discrets périodiques dus au tramage de la grille de sous-pixels (effet de Moiré), tandis que le visage humain présente une décroissance continue en loi de puissance $1/f^\alpha$.

### 3. Défis Actifs de Liveness (EAR et Pose 3D solvePnP)

1. **Rapport d'Aspect Oculaire (Eye Aspect Ratio - EAR)** :
   Pour 6 points de contour oculaire $[p_1, \dots, p_6]$ :
   $$\text{EAR} = \frac{\|p_2 - p_6\|_2 + \|p_3 - p_5\|_2}{2 \|p_1 - p_4\|_2}$$
   Un clignement est validé si $\text{EAR} < 0.20$ pendant au moins 2 trames consécutives suivi d'un retour à $\text{EAR} \ge 0.28$.
2. **Estimation de Pose 3D par solvePnP** :
   À partir du modèle 3D anthropométrique moyen facial $\mathbf{X}_i \in \mathbb{R}^3$ et des points projetés 2D $\mathbf{x}_i \in \mathbb{R}^2$, les angles d'Euler (lacet, tangage, roulis) sont obtenus par la résolution de la projection projective :
   $$s \begin{bmatrix} u \\ v \\ 1 \end{bmatrix} = K \begin{bmatrix} R & \mathbf{T} \end{bmatrix} \begin{bmatrix} X \\ Y \\ Z \\ 1 \end{bmatrix}$$
   où $K$ est la matrice intrinsèque de la caméra et $R \in SO(3)$ la matrice de rotation. Le système valide que l'utilisateur tourne la tête selon l'angle aléatoire demandé par le défi.

### 4. Extraction d'Embedding 512D et Distance Cosinus (EdgeFace-XS)

Le réseau EdgeFace-XS génère un embedding facial $\mathbf{v} \in \mathbb{R}^{512}$ normalisé unitaire :

$$\hat{\mathbf{v}} = \frac{\mathbf{v}}{\|\mathbf{v}\|_2}, \quad \|\hat{\mathbf{v}}\|_2 = 1.0$$

La distance cosinus entre l'échantillon d'enrôlement $\hat{\mathbf{u}}$ et la sonde $\hat{\mathbf{v}}$ est définie par :

$$d_{\text{cos}}(\hat{\mathbf{u}}, \hat{\mathbf{v}}) = 1 - \langle \hat{\mathbf{u}}, \hat{\mathbf{v}} \rangle = 1 - \sum_{i=1}^{512} \hat{u}_i \cdot \hat{v}_i$$

- **Seuil d'authentification** : $d_{\text{cos}} \le 0.40 \iff \text{Similarité Cosinus } \ge 0.60$ (correspondant à un taux de fausse acceptation $\text{FAR} < 10^{-5}$ sur le banc d'essai international LFW avec une précision de 99.73%).

### 5. Biohashing Annulable par Projections Orthogonales Salées

Pour interdire le stockage de $\hat{\mathbf{v}}$ en clair, Hakille projette l'embedding sur un espace binaire de dimension $m = 1024$ bits :

1. Soit un sel secret de 256 bits $S_{\text{bio}}$ stocké dans le trousseau chiffré de l'utilisateur.
2. Une matrice pseudo-aléatoire de projection orthogonale $M_{\text{proj}} \in \mathbb{R}^{m \times 512}$ est générée de façon déterministe par un flux d'octets issu de $\text{HKDF-Expand}(K_{\text{bio}}, S_{\text{bio}})$.
3. Le vecteur binaire de Biohash $\mathbf{b} \in \{0, 1\}^m$ est calculé par seuillage de signe :
   $$b_k = \begin{cases} 1 & \text{si } \sum_{j=1}^{512} M_{k, j} \cdot \hat{v}_j \ge 0 \\ 0 & \text{sinon} \end{cases} \quad \forall k \in [1, m]$$
4. La comparaison de deux gabarits biohashés $\mathbf{b}_1, \mathbf{b}_2$ utilise la distance normalisée de Hamming :
   $$D_H(\mathbf{b}_1, \mathbf{b}_2) = \frac{1}{m} \sum_{k=1}^m (b_{1, k} \oplus b_{2, k})$$
   La correspondance est validée si $D_H \le 0.18$.

**Propriété d'Annulabilité** : Si le sel $S_{\text{bio}}$ est compromis, il suffit de régénérer un nouveau sel $S'_{\text{bio}}$. Les anciens gabarits deviennent instantanément incompatibles avec le nouveau, révoquant l'identité biométrique sans nécessiter de modification physique du visage de l'utilisateur.

### 6. Moteur de Fusion Multimodale et Décroissance Continue

Soit un vecteur de signaux faibles observés à l'instant $t$ :

$$S_t = \{ (m_i, s_i, \omega_i) \} \quad \text{avec } m_i \in \{\text{Face}, \text{Voice}, \text{ProximityBLE}, \text{Keystroke}\}$$

Poids calibrés : $\omega_{\text{Face}} = 0.50$, $\omega_{\text{Voice}} = 0.35$, $\omega_{\text{Proximity}} = 0.15$.
Le score de fusion instantané est :

$$\tau_{\text{inst}} = \frac{\sum_i \omega_i \cdot s_i}{\sum_i \omega_i}$$

En continu, la confiance de la session décroît de manière exponentielle en l'absence de ré-observation :

$$\tau(t + \Delta t) = \tau(t) \cdot 2^{-\frac{\Delta t}{t_{1/2}}}$$

où la demi-vie $t_{1/2} = 180\,\text{secondes}$ (3 minutes).
- **Règle Fail-Closed** : Si $\Delta t \ge 900\,\text{secondes}$ (15 minutes sans aucun signal valide), la session est révoquée unilatéralement et le système bascule en état verrouillé.
- **Élévation de Privilège (*Step-Up Auth*)** : Toute action critique (e.g. signature de transaction financière ou export de trousseau) exige $\tau \ge 0.90$. Si $\tau < 0.90$, un défi actif liveness est immédiatement déclenché.

---

## Modèle de Menace (ISO/IEC 30107-3 Presentation Attack Detection)

| Vecteur d'Attaque PAD | Type d'Attaque (ISO/IEC 30107-3) | Mesure de Détection & Invariant Hakille |
| :--- | :--- | :--- |
| **Photo Imprimée 2D** | Matériel papier couché ou photo numérique haute résolution. | Analyse de texture cutanée par MiniFASNetV2 (absence de micro-pores vivants) + défi actif de clignement (EAR < 0.20). |
| **Rejeu sur Écran 4K/8K** | Affichage d'une vidéo pré-enregistrée sur tablette OLED ou smartphone. | Détection spectrale de Fourier 2D (effet de moiré et tramage sous-pixel) + défi d'angles de pose solvePnP aléatoires non prévisibles. |
| **Masque 3D en Silicone / Latex** | Masque moulé épousant le relief du visage. | Incompatibilité de réflectance spectrale cutanée + suivi du regard par défi de poursuite pupillaire dynamique. |
| **Deepfake Vidéo en Temps Réel** | Remplacement de visage par réseau antagoniste génératif ou diffusion latente. | Défis actifs stochastiques à fenêtre temporelle ultra-courte (10 secondes) avec vérification de nonce cryptographique et cohérence géométrique 3D. |
| **Utilisateur Endormi ou Contraint** | Tentative de déverrouillage devant le visage d'un utilisateur inconscient. | Détection de l'ouverture des yeux (EAR minimal) et exigence d'une réaction volontaire aux challenges actifs. |
| **Inversion de Gabarit (Hill-Climbing)** | Tentative de reconstruction de l'image du visage à partir des vecteurs stockés. | Élimination de tout stockage d'embedding brut sur disque : utilisation exclusive de Biohashing salé irréversible et d'extracteurs flous. |

---

## Contrats d'Interface Rust

### Détection et Extraction Faciale (`hakille-auth::face`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FaceBoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FaceLandmarks {
    pub points: [(f32, f32); 5],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FaceEmbedding {
    pub vector: [f32; 512],
}

impl FaceEmbedding {
    pub fn cosine_distance(&self, other: &FaceEmbedding) -> f32;
    pub fn is_match(&self, reference: &FaceEmbedding, threshold: f32) -> bool;
}

pub struct ScrfdDetector;
impl ScrfdDetector {
    pub fn detect_faces(&self, rgb_frame: &[u8], width: u32, height: u32) -> Result<Vec<(FaceBoundingBox, FaceLandmarks)>, HakilleError>;
}
```

### Détection du Vivant Passive et Active (`hakille-auth::face_liveness`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PassiveSpoofingType {
    None,
    PrintAttack2D,
    ScreenReplayAttack,
    SyntheticDeepfake,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassiveLivenessVerdict {
    pub is_live: bool,
    pub liveness_confidence: f32,
    pub detected_attack: PassiveSpoofingType,
    pub explanation: String,
}

pub struct MiniFasNetClassifier {
    pub fn evaluate_crop(&self, rgb_crop: &[u8], width: u32, height: u32) -> PassiveLivenessVerdict;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChallengeType {
    BlinkEyes,
    GazePursuit { target_direction: (f32, f32) },
    HeadPoseYaw { target_angle_degrees: f32 },
}

pub struct ActiveLivenessChallenge {
    pub challenge_id: [u8; 16],
    pub challenge_type: ChallengeType,
    pub nonce: [u8; 16],
    pub expires_at_timestamp: i64,
}
```

### Protection des Gabarits et Biohashing (`hakille-auth::biohash`)

```rust
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct RawBiometricBuffer {
    pub data: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BiohashDigest {
    pub bits: Vec<u8>,
}

pub struct BiohashingEngine {
    pub fn project_embedding(&self, embedding: &FaceEmbedding, salt: &[u8; 32]) -> BiohashDigest;
    pub fn verify_biohash(&self, probe: &BiohashDigest, reference: &BiohashDigest) -> bool;
}
```

### Fusion Multimodale Continue (`hakille-auth::multimodal`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignalModality {
    FaceRecognition,
    VoicePrint,
    ProximityBle,
    KeystrokeDynamics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultimodalSignal {
    pub modality: SignalModality,
    pub score: f32,
    pub weight: f32,
    pub timestamp: i64,
}

pub struct ContinuousMultimodalFusion {
    pub fn fuse_signals(&mut self, signals: &[MultimodalSignal]) -> f32;
    pub fn apply_temporal_decay(&mut self, elapsed_seconds: f32, half_life_seconds: f32) -> f32;
    pub fn evaluate_privilege_escalation(&self, required_confidence: f32) -> Result<(), HakilleError>;
}
```

---

## Conformité Réglementaire et Éthique

### 1. RGPD Article 9 (Données Biométriques)
- **Consentement Explicite (Opt-In)** : Aucun traitement biométrique ne peut démarrer sans consentement tracé et signé dans `GdprConsentEngine`.
- **Crypto-Shredding Immédiat** : En cas de retrait du consentement, la sous-clé $K_{\text{bio}}$ et le sel $S_{\text{bio}}$ sont détruits, rendant les gabarits mathématiquement irrécupérables en $O(1)$.
- **Zéro Clé Étrangère** : Aucun transfert vers un serveur distant n'est effectué ; le traitement est 100% confiné au matériel local.

### 2. Règlement Européen sur l'IA (EU AI Act - Règlement 2024/1689)
- **Exemption Annexe III Point 1(a)** : Le pipeline est restreint à l'authentification 1:1 sur équipement personnel sous le contrôle direct de l'utilisateur.
- **Interdiction Formelle de Surveillance 1:N** : L'architecture interdit structurellement l'identification biométrique à distance dans les espaces publics ainsi que le profilage émotionnel ou de vulnérabilité non consenti.
