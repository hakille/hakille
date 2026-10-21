# Spécification Complète des Substrates OS et Abstraction Matérielle

> *"Un seul cœur cognitif, six systèmes hôtes, des garanties explicites plutôt qu'une sûreté supposée."*

Ce document décrit le contrat d'abstraction `Substrate`, le trait `Compute` et les
implémentations des six cibles. **État v1 :** plusieurs mécanismes de sandbox
sont décrits ou simulés, mais ne sont pas appliqués de bout en bout ; les
attestations sont des signatures locales de développement, pas des preuves
TPM/Enclave. Les deux agents WASM de référence sont exécutés par le démon dans Wasmtime ; les cinq agents métier ne le sont pas. La matrice finale
distingue la cible de l'état vérifié.

---

## 1. Contrats d'Abstraction Fondamentaux : Traits `Substrate` et `Compute`

Dans Hakille, le noyau cognitif et le graphe de mémoire passent par les traits Rust du substrate plutôt que d'appeler directement les syscalls. Les agents métier, non exécutés en v1, seraient eux aussi confinés derrière ce contrat. Les traits sont définis dans [`crates/hakille-substrate`](../crates/hakille-substrate) :

### Le Trait `Substrate` : Contrôle Système et Environnement
Le trait `Substrate` formalise toutes les capacités requises par l'OS cognitif :

```rust
#[async_trait::async_trait]
pub trait Substrate: Send + Sync + 'static {
    /// Retourne le manifeste statique et dynamique des capacités de la plateforme
    fn manifest(&self) -> &PlatformManifest;

    /// Signe une attestation de plateforme (v1 : clé locale de dev, pas de TPM)
    async fn attest(&self, data: &[u8]) -> hakille_core::Result<SignedAttestation>;

    // Système de fichiers restreint par capability Biscuit
    async fn read_file(&self, path: &VirtualPath, cap: &CapabilityToken) -> hakille_core::Result<Vec<u8>>;
    async fn write_file(&self, path: &VirtualPath, data: &[u8], cap: &CapabilityToken) -> hakille_core::Result<()>;
    async fn list_dir(&self, path: &VirtualPath, cap: &CapabilityToken) -> hakille_core::Result<Vec<VirtualPath>>;
    async fn delete_file(&self, path: &VirtualPath, cap: &CapabilityToken) -> hakille_core::Result<()>;

    // Réseau restreint par capability Biscuit
    async fn http_request(&self, req: HttpRequest, cap: &CapabilityToken) -> hakille_core::Result<HttpResponse>;

    // Audio physique (streaming & playback)
    async fn start_audio_capture(&self, config: AudioConfig, cap: &CapabilityToken) -> hakille_core::Result<AudioStreamHandle>;
    async fn stop_audio_capture(&self, handle: AudioStreamHandle) -> hakille_core::Result<()>;
    async fn play_audio(&self, data: &[u8], config: AudioConfig, cap: &CapabilityToken) -> hakille_core::Result<()>;

    // Flux caméra pour perception visuelle (sans fuite de pixels bruts)
    async fn start_camera_capture(&self, config: CameraConfig, cap: &CapabilityToken) -> hakille_core::Result<CameraStreamHandle>;
    async fn stop_camera_capture(&self, handle: CameraStreamHandle) -> hakille_core::Result<()>;
    async fn capture_camera_frame(&self, handle: CameraStreamHandle, cap: &CapabilityToken) -> hakille_core::Result<CameraFrame>;

    // Affichage et injection d'entrées physiques
    async fn capture_screen(&self, cap: &CapabilityToken) -> hakille_core::Result<ScreenCapture>;
    async fn inject_input(&self, action: InputAction, cap: &CapabilityToken) -> hakille_core::Result<()>;

    // Confinement de processus WASM
    async fn spawn_sandboxed(&self, wasm: &[u8], caps: Vec<CapabilityToken>) -> hakille_core::Result<ProcessHandle>;

    // Télémétrie d'alimentation et thermique
    fn power_state(&self) -> PowerState;
    fn thermal_state(&self) -> ThermalState;
}
```

### Le Trait `Compute` : Accélération Neuronale Orthogonale
L'accélération matérielle (CUDA, Metal, Vulkan, NPU, WebGPU) est strictement découplée de l'OS sous-jacent. Un système Linux peut tourner sur CPU ou GPU CUDA ; macOS utilise Apple Silicon Metal ; le Web utilise WebGPU :

```rust
pub trait Compute: Send + Sync + 'static {
    /// Liste des backends d'accélération disponibles sur la machine
    fn available_backends(&self) -> Vec<ComputeBackend>;

    /// Backend recommandé par défaut pour la plateforme
    fn preferred_backend(&self) -> ComputeBackend;

    /// Mémoire VRAM allouable en octets
    fn vram_available_bytes(&self) -> u64;

    /// Mémoire vive physique immédiatement allouable en octets
    fn ram_available_bytes(&self) -> u64;

    /// Vérifie le support d'une quantification neuronale (ex: "q4_k", "int8", "fp16")
    fn supports_quantization(&self, quant: &str) -> bool;
}
```

---

## 2. Implémentations Natives des 6 Systèmes d'Exploitation

### 2.1. Windows (`hakille-substrate-windows`) — Tier Natif

L'implémentation Windows s'interface directement avec l'API Win32 via des liaisons FFI C sûres, sans runtime externe.

- **Capture d'Écran Réelle (GDI BitBlt)** :
  Utilise `GetDC(NULL)`, `CreateCompatibleDC`, `CreateCompatibleBitmap`, et `BitBlt` avec `SRCCOPY`. Récupère les dimensions réelles de l'écran via `GetSystemMetrics(SM_CXSCREEN)` et `GetSystemMetrics(SM_CYSCREEN)`. Extrait le tampon brut de pixels 32 bits BGRA/RGBA via `GetDIBits` avec `BI_RGB`.
- **Injection d'Entrées Physiques (`SendInput`)** :
  Convertit les intentions `InputAction` en structures natives Win32 `INPUT` (`INPUT_MOUSE`, `INPUT_KEYBOARD`). Calcule les coordonnées absolues normalisées $0..65535$ avec les flags `MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE`, `MOUSEEVENTF_LEFTDOWN`, `MOUSEEVENTF_LEFTUP`. Pour le clavier, gère les codes virtuels `VK_*` et les flags `KEYEVENTF_KEYUP`.
- **Isolation de Processus AppContainer** :
  Configure un profil sandbox AppContainer (`Hakille.AppContainer.<Nom>`) avec des SIDs de capabilities déclarées (ex: `S-1-15-3-1` pour `NetworkClient`, `S-1-15-3-2` pour `NetworkClientServer`). Crée des ACLs strictes sur le système de fichiers interdisant l'accès en dehors du répertoire racine attribué à l'agent.
- **Télémétrie Matérielle Réelle** :
  - Mémoire vive interrogée via `GlobalMemoryStatusEx` (`MEMORYSTATUSEX`), extrayant `ullTotalPhys`, `ullAvailPhys` et `dwMemoryLoad`.
  - Alimentation interrogée via `GetSystemPowerStatus` (`SYSTEM_POWER_STATUS`), extrayant `ACLineStatus` (secteur vs batterie), `BatteryLifePercent` et `BatteryFlag`.
- **Présence Système Win32** :
  - Icône dans la zone de notification (System Tray) via `Shell_NotifyIconW` avec menu contextuel minimal.
  - Raccourci global de réveil `Alt + Space` configuré via `RegisterHotKey` pour déployer instantanément le HUD Spotlight éphémère.

---

### 2.2. Linux (`hakille-substrate-linux`) — Tier Natif

L'implémentation Linux exploite les primitives modernes de sécurité du noyau Linux pour un contrôle granulaire sans failles.

- **Filtres Seccomp-BPF Natifs et Simulateur de Bytecode** :
  Génère des instructions de filtrage Berkeley Packet Filter (BPF) strictes (`sock_filter`, `sock_fprog`) avec validation de l'architecture (`SyscallArch::X86_64` vs `Aarch64`).
  - *Profil WASM Sandbox* : autorise uniquement `mmap`, `munmap`, `mprotect`, `futex`, `read`, `write`, `exit_group` ; retourne `SECCOMP_RET_ERRNO` sur tout autre appel système.
  - *Profil Strict Lockdown* : retourne immédiatement `SECCOMP_RET_KILL_PROCESS` sur toute tentative d'appel système non autorisé.
  - *Simulateur BPF intégré* : vérifie le bytecode généré avant son application (`SeccompProfile::simulate_bpf`), prouvant statiquement la conformité de la table des filtres.
- **Isolation par Namespaces Linux** :
  Isole chaque agent WASM dans des espaces de noms dédiés :
  - `CLONE_NEWPID` : l'agent ne voit que son propre sous-arbre de processus.
  - `CLONE_NEWNS` (Mount) : montage d'un système de fichiers racine chroot en lecture seule avec point de montage éphémère `/tmp` en `tmpfs`.
  - `CLONE_NEWNET` : pas d'interface réseau sauf interface `lo` ou veth isolée.
  - `CLONE_NEWIPC` et `CLONE_NEWUTS` : isolation totale des IPC et du nom d'hôte.
- **Contrôleur de Ressources Cgroups v2** :
  Applique des quotas stricts sur `/sys/fs/cgroup/hakille/` : `memory.max` (borne stricte de RAM), `cpu.max` (quota de cycles processeur) et `pids.max` (prévention des fork-bombs).
- **Injection d'Entrées Virtuelles via `/dev/uinput`** :
  Crée un périphérique virtuel `uinput` (`UI_DEV_CREATE`, `UI_DEV_SETUP`) capable d'émettre des événements `EV_KEY`, `EV_REL`, `EV_ABS` et `EV_SYN` conformes au protocole Linux Input Subsystem.
- **Capture d'Écran Wayland & X11** :
  Supporte le protocole Wayland via `xdg-desktop-portal` et ScreenCast D-Bus, avec repli natif sur `XGetImage` sous les sessions X11 classiques.
- **Télémétrie `/proc` et `/sys`** :
  Lit directement `/proc/meminfo` (`MemTotal`, `MemAvailable`) et `/sys/class/power_supply/` pour un coût CPU proche de zéro.

---

### 2.3. macOS (`hakille-substrate-macos`) — Tier Natif

L'implémentation macOS s'intègre étroitement à l'architecture Apple Silicon et aux frameworks de sécurité du système Darwin.

- **Compilation de Profils Sandbox Seatbelt (`(deny default)`)** :
  Génère des profils de confinement en syntaxe S-Expression Scheme pour le sous-système Sandbox de macOS.
  - Par défaut : `(deny default)`, bloquant tout accès disque, réseau, IPC et entrées.
  - Déclarations ciblées : autorise `(allow file-read* (subpath ...))` sur le dossier de travail de l'agent, bloque `(deny network*)` pour les agents locaux, et autorise l'accès direct aux accélérateurs Metal (`(allow iokit-open (iokit-user-client-class "com.apple.metal.device"))`).
  - Application via `sandbox_init` / API Seatbelt avec validation d'intégrité statique.
- **Interception Endpoint Security Framework** :
  Client `EndpointSecurityClient` configuré pour intercepter les événements système (`ES_EVENT_TYPE_AUTH_EXEC`, `ES_EVENT_TYPE_NOTIFY_OPEN`). Gère une liste blanche de chemins système muets (`mute_paths`) pour éliminer le bruit des démons macOS (`loginwindow`, `mds`, `launchd`).
- **Télémétrie Mach et IOKit** :
  - Mémoire vive Apple Silicon : interroge le port hôte Mach via `mach_host_self()` et `host_statistics64(HOST_VM_INFO64)`, extrayant la mémoire libre, active, inactive, et la mémoire câblée (*wired memory* réservée aux modèles neuronaux unifiés).
  - Énergie : interroge le dictionnaire `IOPMPowerSource` via le framework `IOKit` pour déterminer l'état de charge de la batterie et le branchement MagSafe.
- **Capture d'Écran Retina 2x via CoreGraphics** :
  Utilise `CGWindowListCreateImage` avec `kCGWindowListOptionOnScreenOnly` et l'espace colorimétrique `CGColorSpaceCreateDeviceRGB()`. Prend en compte le facteur d'échelle Retina 2x pour fournir des coordonnées au pixel près à l'agent UI-TARS.

---

### 2.4. Redox OS (`hakille-substrate-redox`) — Tier Souverain

Redox OS représente la cible d'accueil souveraine d'Hakille : un système d'exploitation micro-noyau entièrement écrit en Rust où chaque ressource est adressable sous forme d'URI de *scheme* (`scheme:`).

- **Modèle Unifié Tout-en-Scheme** :
  Hakille ne charge pas de pilotes monolithiques mais implémente et consomme les schemes natifs du micro-noyau :
  - `chan:` : Scheme de canaux IPC bidirectionnels à haute vitesse entre le noyau cognitif et les agents WASM.
  - `orbital:` : Scheme du gestionnaire de fenêtres Orbital. Crée des surfaces d'affichage mémoire partagée (`orbital:Hakille_Overlay`) pour projeter le HUD Spotlight et les indicateurs sans passer par un serveur X11/Wayland.
  - `disk:` : Scheme d'accès direct au système de fichiers RedoxFS avec permissions cryptographiques.
  - `sys:` : Télémétrie native exposée par le kernel Redox (`sys:memory`, `sys:cpu`, `sys:context`).
  - `audio:` : Accès brut aux flux PCM du pilote audio AC'97/Intel HDA.
- **Sérialisation et Routage `RedoxPacket`** :
  Toutes les requêtes traversent la boucle de traitement de paquets micro-noyau :
  ```rust
  pub struct RedoxPacket {
      pub id: u64,
      pub op: usize, // SYS_OPEN, SYS_READ, SYS_WRITE, SYS_CLOSE, SYS_FEVENT
      pub a: usize,
      pub b: usize,
      pub c: usize,
      pub d: usize,
  }
  ```
  Le gestionnaire `RedoxSchemeHandler` dispatche chaque opération de façon non bloquante avec un espace de mémoire partagée préalloué.
- **Délégation de Capacités Microkernel** :
  Gère une table interne de descripteurs de capacités (`RedoxCapabilityTable`) : `CAP_READ`, `CAP_WRITE`, `CAP_EXEC`. L'accès à une ressource sans bit de capability valide retourne immédiatement `SubstrateError::PermissionDenied`.

---

### 2.5. Mobile (`hakille-substrate-mobile`) — Tier Invité

Les plateformes Android et iOS interdisent l'exécution de démons en arrière-plan permanent et limitent l'accès aux périphériques. L'implémentation mobile contourne ces contraintes via le concept d'**Ombre Cognitive** (*Shadow Node*).

- **Synchronisation d'Ombre (*Shadow Sync*) avec Horloge de Lamport** :
  Une instance légère d'Hakille tourne en continu sur le hub domestique ou le cluster TiKV. Lorsque le terminal mobile se connecte, le moteur `MobileShadowSync` réconcilie l'état local avec l'ombre distante selon la règle Last-Write-Wins (LWW) et les horloges de Lamport.
  - Gestion des conflits hors-ligne : les modifications locales en file d'attente (`queue_local_delta`) sont propagées au réveil.
  - Tombstones de suppression : les suppressions locales créent un delta marqué `is_deleted = true`, empêchant les résurrections de données obsolètes.
- **Machine à États d'Économie d'Énergie (*Battery Conservation*)** :
  Le substrate mobile bascule dynamiquement entre quatre états :
  1. `ActiveStreaming` : App au premier plan, streaming audio et websocket ouverts.
  2. `SuspendedDozing` : App mise en veille par l'OS (Android Doze / iOS Background Suspension) ; arrêt de l'écoute active.
  3. `ReconnectingBackoff` : Perte de réseau ; temporisation exponentielle ($2^n \times 100\text{ ms}$) pour préserver la batterie.
  4. `Disconnected` : Mode avion ou hors-ligne complet.
- **Stockage Scopé (Scoped Storage)** :
  Respecte les contraintes Android SAF (Storage Access Framework) et iOS App Sandbox : les fichiers ne peuvent être écrits que dans le conteneur sandbox privé attribué par l'OS.

---

### 2.6. Web (`hakille-substrate-web`) — Tier Invité

L'implémentation Web permet à Hakille de tourner dans n'importe quel navigateur moderne via WebAssembly (WASM).

- **Bridge Web Audio et Rééchantillonnage Linéaire** :
  Les microphones des navigateurs opèrent généralement à 44.1 kHz ou 48 kHz, tandis que les modèles ASR (SenseVoice-Small) et le codec Mimi exigent un flux PCM 16 kHz.
  - `WebAudioBridge` intègre un rééchantillonneur linéaire continu réduisant 48 kHz en 16 kHz (ratio 3:1).
  - Tampon circulaire anti-overrun : limite la taille du tampon audio à 200 ms (3200 échantillons) pour éviter toute accumulation de latence ou débordement mémoire dans le navigateur.
- **Capture d'Écran Canvas ImageData** :
  Ingère des trames visuelles provenant d'un élément `<canvas>` HTML5 ou de l'API `getDisplayMedia()` sous forme de tampons de pixels RGBA normalisés.
- **Détection des Capacités WebGPU** :
  Inspecte l'adaptateur `GPUAdapter` du navigateur via les liaisons WebGPU :
  - Détecte les limites de groupe de travail (`max_compute_workgroup_storage_size`, `max_compute_invocations_per_workgroup`).
  - Valide la disponibilité des shaders de calcul pour exécuter des tenseurs quantifiés INT8 ou Q4 directement sur le GPU de l'utilisateur.

---

## 3. Matrice Comparative des Paliers (cible et état v1)

| Critère | Windows (`windows`) | Linux (`linux`) | macOS (`macos`) | Redox OS (`redox`) | Mobile (`mobile`) | Web (`web`) |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Palier (Tier)** | Native | Native | Native | **Sovereign** | Guest | Guest |
| **Niveau de Contrôle** | Élevé (Win32) | Quasi total | Quasi total | Élevé (schemes) | Restreint (OS) | Minimal (Browser) |
| **Mécanisme de Sandbox (v1)** | AppContainer décrit, enforcement non branché | Seccomp simulé + namespaces décrits | Profils Seatbelt générés, application non validée | Schemes en-process, partiel | Scoped storage décrit | Wasm Sandbox navigateur |
| **Injection d'Entrées** | `SendInput` | `/dev/uinput` | CGEventCreateMouseEvent | Schemes `orbital:` | Non supporté | Événements DOM |
| **Capture d'Écran** | GDI BitBlt | Wayland Portal / X11 | CoreGraphics Retina 2x | Surface `orbital:` | Délégué au Hub | Canvas ImageData |
| **Attestation (v1)** | Signature dev locale | Signature dev locale | Signature dev locale | Signature dev locale | Signature dev locale | Non disponible |
| **Gestion Alimentation** | `GetSystemPowerStatus` | `/sys/class/power_supply` | IOKit IOPMPowerSource | `sys:power` | BatteryManager | Battery API (opt) |
| **Continuité de Session** | Locale persistante | Locale persistante | Locale persistante | Locale persistante | **Shadow Sync TiKV** | Déportée Hub |
