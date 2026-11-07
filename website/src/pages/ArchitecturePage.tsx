import { lazy, Suspense } from 'react';
import { WORKSPACE_METRICS } from '@/data/cratesCatalog';
import { HARDWARE_TIERS } from '@/data/substratesData';
import { pickBi } from '@/i18n/types';
import { useI18n } from '@/i18n/context';
import { PrintDossierButton } from '@/components/primitives/PrintDossierButton';
import { SimulatorSkeleton } from '@/components/common';

const ArchitectureAndFabricExplorer = lazy(() =>
  import('@/components/simulators/ArchitectureAndFabricExplorer').then((m) => ({
    default: m.ArchitectureAndFabricExplorer,
  }))
);
const CognitiveNotificationRouterLab = lazy(() =>
  import('@/components/simulators/CognitiveNotificationRouterLab').then((m) => ({
    default: m.CognitiveNotificationRouterLab,
  }))
);

const ALL_14_ADRS = [
  {
    id: 'ADR-001',
    titleFr: '100 % Rust Édition 2024 & Zéro Runtime Python',
    titleEn: 'Pure Rust 2024 & Zero Python Runtime',
    detailFr: 'Élimine le GIL, la dérive pip/conda et les failles mémoire C++ sur les 42 crates.',
    detailEn: 'Eliminates GIL, pip/conda drift, and C++ glue vulnerabilities across all 42 crates.',
  },
  {
    id: 'ADR-002',
    titleFr: 'Capabilities Biscuit v5 au lieu de Permissions Ambiantes',
    titleEn: 'Biscuit v5 Capabilities Over Ambient Permissions',
    detailFr: 'Chaque sous-agent reçoit un jeton Datalog signé qui ne peut que restreindre ses droits.',
    detailEn: 'Every sub-agent receives a Datalog token that can only attenuate rights monotonically.',
  },
  {
    id: 'ADR-003',
    titleFr: 'Graphe SPO sur redb ACID + Index Vectoriel HNSW',
    titleEn: 'SPO Graph on redb ACID + HNSW Vector Index',
    detailFr: 'Votre identité réside dans des triplets inspectables et rectifiables, jamais figée dans les poids.',
    detailEn: 'Identity lives in explicit auditable triples, never baked into opaque neural weights.',
  },
  {
    id: 'ADR-004',
    titleFr: 'Pipeline Audio Full-Duplex Candle & Codec Mimi 24 kHz',
    titleEn: 'Full-Duplex Audio Pipeline on Candle & Mimi 24 kHz',
    detailFr: 'Streaming audio 24 kHz sans serveur Python avec interruption Barge-In < 80 ms.',
    detailEn: '24 kHz streaming audio without Python servers and < 80 ms Barge-In interruption.',
  },
  {
    id: 'ADR-005',
    titleFr: 'Abstraction de Substrat OS (Linux, macOS, Windows, Redox, Mobile, Web)',
    titleEn: 'Per-OS Substrate Abstraction Layer',
    detailFr: 'Sépare proprement le noyau cognitif des primitives système de chaque hôte.',
    detailEn: 'Cleanly isolates cognitive invariants from host-specific kernel primitives.',
  },
  {
    id: 'ADR-006',
    titleFr: 'Synchronisation CRDT Chiffrée de Bout en Bout',
    titleEn: 'End-to-End Encrypted CRDT Multi-Device Sync',
    detailFr: 'Réplication locale entre vos équipements sans jamais exposer le clair à un relais tiers.',
    detailEn: 'Device-to-device state convergence with zero plaintext exposure to relay servers.',
  },
  {
    id: 'ADR-007 / 008',
    titleFr: 'Gel de Slint & Spikes Iced — Leçon d’Ingénierie Documentée',
    titleEn: 'Slint Freeze & Iced Spikes — Documented UI Tradeoffs',
    detailFr: 'Conservation transparente des prototypes évalués avant le choix d’un moteur natif sur mesure.',
    detailEn: 'Open preservation of evaluated UI toolkits prior to building the custom native renderer.',
  },
  {
    id: 'ADR-009',
    titleFr: 'Shell Natif Pur winit + wgpu + AccessKit (< 25 Mo RAM)',
    titleEn: 'Native winit + wgpu + AccessKit GPU Shell (< 25 MB RAM)',
    detailFr: '0 % Electron/WebView, 0.0 fps garanti au repos et arbre d’accessibilité OS natif complet.',
    detailEn: '0% Electron/WebView overhead, true 0.0 fps idle suspension, and full OS Accessibility tree.',
  },
  {
    id: 'ADR-010',
    titleFr: 'Compatibilité Explicite par OS & Matrice des 3 Paliers',
    titleEn: 'Explicit Per-OS Compatibility & 3-Tier Honesty Matrix',
    detailFr: 'Ne promet jamais sur mobile ou navigateur une isolation noyau réservée à Linux/Redox.',
    detailEn: 'Never claims kernel-level guarantees on guest OS platforms that forbid them.',
  },
  {
    id: 'ADR-011',
    titleFr: 'Compagnon Mobile Natif (SwiftUI / Compose) & Checkpoint v2',
    titleEn: 'Native Mobile Companion & Durable Checkpoint v2',
    detailFr: 'Pont UniFFI 0.32.2 marquant « unknown » toute mutation interrompue pour interdire le double envoi.',
    detailEn: 'UniFFI 0.32.2 bridge marking interrupted mutations "unknown" to prevent duplicate side effects.',
  },
  {
    id: 'ADR-012',
    titleFr: 'Stockage Zero-Knowledge, Indexation Aveugle & Crémation Cryptographique',
    titleEn: 'Zero-Knowledge Storage, Blind Indexing & Cryptographic Cremation',
    detailFr: 'Chiffrement XChaCha20-Poly1305 lié au contexte (AAD), sous-clés HKDF ZeroizeOnDrop, index aveugle HMAC et partage Shamir.',
    detailEn: 'Context-bound XChaCha20-Poly1305 AEAD, ZeroizeOnDrop HKDF subkeys, HMAC blind indexing, and Shamir social recovery.',
  },
  {
    id: 'ADR-013',
    titleFr: 'Multi-Tenancy à 5 Niveaux, Moteur Hybride ReBAC + Biscuit & Délégation',
    titleEn: '5-Tier Multi-Tenancy, Hybrid ReBAC + Biscuit & Sovereign Agent Delegation',
    detailFr: 'Sphères Enterprise, Org, Group, Team et Family (invariant NoWallet pour mineurs, sessions invités <= 24 h).',
    detailEn: 'Enterprise, Org, Group, Team, and Family isolation spheres (minor NoWallet invariant, <= 24h bounded guest sessions).',
  },
  {
    id: 'ADR-014',
    titleFr: 'Biométrie Faciale Révocable BioHash, PAD ISO/IEC 30107-3 & Fusion Continue',
    titleEn: 'Revocable BioHash Face Auth, ISO/IEC 30107-3 Liveness PAD & Multimodal Fusion',
    detailFr: 'Anti-usurpation MiniFASNetV2 + défi actif, gabarits BioHash révocables (0 image brute sur disque) et fusion voix+visage.',
    detailEn: 'MiniFASNetV2 passive PAD + active challenge-response, revocable BioHash projections (0 raw face on disk), and voice+face fusion.',
  },
] as const;

export function ArchitecturePage() {
  const { locale, t } = useI18n();

  return (
    <div>
      <section className="page-hero">
        <div className="container" style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
          <span className="eyebrow-mono ltr-isolate">
            {WORKSPACE_METRICS.totalPackages} PACKAGES ·{' '}
            {WORKSPACE_METRICS.totalRustLoc.toLocaleString('en-US')} LOC · MSRV {WORKSPACE_METRICS.msrv}
          </span>
          <h1 className="display-h1">{t.routes.architecture.title}</h1>
          <p className="hero-lead">{t.routes.architecture.description}</p>
          <PrintDossierButton />
        </div>
      </section>

      <div className="container page-content">
        <Suspense
          fallback={
            <SimulatorSkeleton
              title={pickBi(locale, 'Explorateur d’Architecture & 42 Crates', 'Architecture Explorer & 42 Crates')}
              description={pickBi(
                locale,
                'Cartographie interactive des 6 couches architecturales et bail Biscuit 300s.',
                'Interactive mapping of 6 architectural layers and 300s Biscuit lease.'
              )}
              minHeight={560}
            />
          }
        >
          <ArchitectureAndFabricExplorer />
        </Suspense>

        {/* Couche 4 & 5 — Sovereign Multiplatform Notification Fabric (SMNF) */}
        <section>
          <div className="section-header">
            <span className="eyebrow-mono">SMNF · CRATES/HAKILLE-SUBSTRATE & HAKILLE-UI</span>
            <h2 className="section-h2">
              {pickBi(
                locale,
                'Fabric de Notifications Multiplateforme Souverain (SMNF) & Routeur Cognitif',
                'Sovereign Multiplatform Notification Fabric (SMNF) & Cognitive Router'
              )}
            </h2>
            <p className="section-lead">
              {pickBi(
                locale,
                'Arbitrage multimodal en temps réel : interception des alertes, protection contre la somnolence cognitive (PERCLOS >= 15 %) et respect inviolable du budget de silence (3 suggestions/jour).',
                'Real-time multimodal arbitration: intercepts notifications, protects against cognitive somnolence (PERCLOS >= 15%), and enforces the daily silence budget (3 suggestions/day).'
              )}
            </p>
          </div>

          <Suspense
            fallback={
              <SimulatorSkeleton
                title={pickBi(
                  locale,
                  'Routeur de Notifications Cognitif SMNF',
                  'SMNF Cognitive Notification Router'
                )}
                description={pickBi(
                  locale,
                  'Arbitrage multimodal en temps réel : regard 3D, score PERCLOS, silence budget et IPC native.',
                  'Real-time multimodal arbitration: 3D gaze, PERCLOS score, silence budget, and native substrate IPC.'
                )}
                minHeight={520}
              />
            }
          >
            <CognitiveNotificationRouterLab />
          </Suspense>
        </section>

        {/* DEEP DIVE SECTION 1: SMNF 6-SUBSTRATE DISPATCH ARCHITECTURE */}
        <section className="arch-deep-dive-section">
          <div className="section-header">
            <span className="eyebrow-mono">CRATES/HAKILLE-SUBSTRATE-* · DISPATCH SPÉCIFIQUE PAR OS</span>
            <h2 className="section-h2">
              {pickBi(
                locale,
                'Architecture de Dispatch SMNF sur les 6 Substrats Hôtes',
                'SMNF 6-Substrate Native Dispatch Architecture'
              )}
            </h2>
            <p className="section-lead">
              {pickBi(
                locale,
                'Le Sovereign Multiplatform Notification Fabric unifie les notifications système sans jamais imposer de service cloud intermédiaire (zéro serveur Firebase / APNs tiers requis en local).',
                'The Sovereign Multiplatform Notification Fabric unifies system notifications across 6 host operating systems without any intermediate cloud relay (zero third-party Firebase/APNs servers required locally).'
              )}
            </p>
          </div>

          <div className="arch-substrate-grid">
            <article className="arch-substrate-card">
              <div className="card-top">
                <span className="platform-tag">Linux</span>
                <span className="mono crate-ref">crates/hakille-substrate-linux</span>
              </div>
              <h3 className="card-title">FreeDesktop D-Bus Notification Protocol</h3>
              <p className="card-desc">
                {pickBi(
                  locale,
                  'Inspecte $DISPLAY ou $WAYLAND_DISPLAY. Transmet les requêtes via le bus de session org.freedesktop.Notifications ou spawn de notify-send avec urgence (low, normal, critical). En mode sans tête, bascule vers linux_session_log.',
                  'Inspects $DISPLAY or $WAYLAND_DISPLAY. Dispatches via org.freedesktop.Notifications session D-Bus or notify-send spawn with mapped urgencies. Headless environments fall back to linux_session_log.'
                )}
              </p>
              <div className="mono tech-spec-snippet">
                <code>notify-send "$title" "$body" -u &lt;urgency&gt; -a "Hakille" -t &lt;timeout_ms&gt;</code>
              </div>
            </article>

            <article className="arch-substrate-card">
              <div className="card-top">
                <span className="platform-tag">macOS Darwin</span>
                <span className="mono crate-ref">crates/hakille-substrate-macos</span>
              </div>
              <h3 className="card-title">Darwin UserNotifications & AppleScript Bridge</h3>
              <p className="card-desc">
                {pickBi(
                  locale,
                  'Exécute l’affichage natif via osascript sur le bridge UserNotifications. Prise en charge des sons natifs et sous-titres, avec persistance dans le Centre de Notifications macOS sans passerelle distante.',
                  'Executes native notifications via osascript over Darwin UserNotifications.framework. Supports system sound chimes and persistent banners without external gateway servers.'
                )}
              </p>
              <div className="mono tech-spec-snippet">
                <code>osascript -e 'display notification "$body" with title "$title" subtitle "Hakille"'</code>
              </div>
            </article>

            <article className="arch-substrate-card">
              <div className="card-top">
                <span className="platform-tag">Windows</span>
                <span className="mono crate-ref">crates/hakille-substrate-windows</span>
              </div>
              <h3 className="card-title">WinRT Toast & Shell_NotifyIconW Fallback</h3>
              <p className="card-desc">
                {pickBi(
                  locale,
                  'Dispatche vers WinRT Toast XML avec drapeaux d’urgence (0x1 Info, 0x2 High, 0x3 Critical). En l’absence d’environnement UWP, bascule sur Shell_NotifyIconW (NIM_ADD) avec bulle de zone de notification système.',
                  'Dispatches to WinRT Toast XML notifications with urgency bitmasks (0x1 Info, 0x2 High, 0x3 Critical). Falls back to Shell_NotifyIconW (NIM_ADD) system tray balloon in classic desktop mode.'
                )}
              </p>
              <div className="mono tech-spec-snippet">
                <code>Shell_NotifyIconW(NIM_ADD, &amp;nid) | Windows.UI.Notifications.ToastNotification</code>
              </div>
            </article>

            <article className="arch-substrate-card">
              <div className="card-top">
                <span className="platform-tag">Mobile Companion</span>
                <span className="mono crate-ref">crates/hakille-substrate-mobile</span>
              </div>
              <h3 className="card-title">MobileShadowSync & Doze Conservation</h3>
              <p className="card-desc">
                {pickBi(
                  locale,
                  'File locale MobileShadowSync respectant le mode Doze d’économie d’énergie Android et l’inactivité iOS. Regroupe les alertes non critiques et ne réveille l’appareil qu’à confirmation ACK ou urgence Critical.',
                  'Local MobileShadowSync queue respecting Android Doze battery conservation. Batches non-critical alerts locally, only waking radios upon explicit ACK confirmation or Critical urgency.'
                )}
              </p>
              <div className="mono tech-spec-snippet">
                <code>MobileShadowSync::enqueue(req) -&gt; wake_on_ack(channel_handle)</code>
              </div>
            </article>

            <article className="arch-substrate-card">
              <div className="card-top">
                <span className="platform-tag">Web Browser</span>
                <span className="mono crate-ref">crates/hakille-substrate-web</span>
              </div>
              <h3 className="card-title">W3C HTML5 Notification API & WebAudio Chime</h3>
              <p className="card-desc">
                {pickBi(
                  locale,
                  'Dispatche via window.Notification et joue un carillon Karplus-Strong synthétisé en local via WebAudio. Si la permission est refusée, le routeur réoriente vers le HUD overlay en canvas sans fuite réseau.',
                  'Dispatches through window.Notification API backed by local Karplus-Strong WebAudio chimes. When permission is denied, gracefully redirects into the in-page canvas overlay without network leaks.'
                )}
              </p>
              <div className="mono tech-spec-snippet">
                <code>new Notification(title, &#123; body, icon &#125;) + AudioContext.createOscillator()</code>
              </div>
            </article>

            <article className="arch-substrate-card">
              <div className="card-top">
                <span className="platform-tag">Redox OS</span>
                <span className="mono crate-ref">crates/hakille-substrate-redox</span>
              </div>
              <h3 className="card-title">Redox Microkernel Schemes (orbital: &amp; chan:)</h3>
              <p className="card-desc">
                {pickBi(
                  locale,
                  'Communication directe avec le serveur graphique de fenêtrage Orbital via le scheme orbital:notify et passage de messages chan: sans IPC monolithique.',
                  'Direct microkernel scheme communication with the Orbital display server via orbital:notify and chan: message passing, eliminating monolithic IPC bloat.'
                )}
              </p>
              <div className="mono tech-spec-snippet">
                <code>File::open("orbital:notify") -&gt; write_all(&amp;ipc_wire_frame)</code>
              </div>
            </article>
          </div>
        </section>

        {/* DEEP DIVE SECTION 2: BISCUIT V5 & CAPABILITY ENGINE */}
        <section className="arch-deep-dive-section">
          <div className="section-header">
            <span className="eyebrow-mono">CRATES/HAKILLE-CORE & HAKILLE-IDENTITY · BISCUIT V5</span>
            <h2 className="section-h2">
              {pickBi(
                locale,
                'Jetons de Capabilities Biscuit v5 & Moteur d’Atténuation Datalog',
                'Biscuit v5 Capability Tokens & Monotonic Datalog Engine'
              )}
            </h2>
            <p className="section-lead">
              {pickBi(
                locale,
                'Hakille rejette catégoriquement le modèle de sécurité ambiant (sudo / tokens porteurs statiques). Tout droit d’exécution découle d’un jeton Datalog formellement vérifié dont chaque délégation est strictement décroissante.',
                'Hakille categorically rejects ambient authority (sudo / bearer API keys). Every operation requires an explicit Biscuit token evaluated against formal Datalog policies where delegation can only narrow privileges.'
              )}
            </p>
          </div>

          <div className="biscuit-breakdown-grid">
            <div className="biscuit-logic-card">
              <h3 className="card-title">
                {pickBi(locale, '1. Canon Datalog & Invariant Monotone', '1. Datalog Canon & Monotonic Invariant')}
              </h3>
              <p className="card-desc">
                {pickBi(
                  locale,
                  'Chaque bloc Biscuit ajoute des faits et des vérifications (checks). Un sous-agent ne peut jamais élargir ses privilèges : toute tentative d’ajouter un scope absent du bloc parent provoque une erreur immédiate HakilleError::AttenuationViolation.',
                  'Each Biscuit block appends immutable facts and checks. Sub-agents can never expand permissions: any attempt to introduce a scope absent from the parent token triggers HakilleError::AttenuationViolation.'
                )}
              </p>
              <div className="mono datalog-box">
                <span className="datalog-comment">// Bloc 0 : Autorité Racine (HAKKEY02)</span><br />
                right(&quot;filesystem&quot;, &quot;read&quot;);<br />
                right(&quot;notification&quot;, &quot;write&quot;);<br />
                check if time($time), $time &lt;= 1799404800;<br />
                <br />
                <span className="datalog-comment">// Bloc 1 : Atténuation Sous-Agent (D_depth = 1)</span><br />
                allowed_scope(&quot;notification&quot;);<br />
                allowed_action(&quot;write&quot;);<br />
                check if operation($s, $a), allowed_scope($s), allowed_action($a);<br />
                check if resource($res), $res == &quot;hud:toast&quot;;
              </div>
            </div>

            <div className="biscuit-logic-card">
              <h3 className="card-title">
                {pickBi(
                  locale,
                  '2. Borne de Profondeur D_max = 4 & Clé Pinned',
                  '2. Bounded Depth D_max = 4 & Pinned Root Key'
                )}
              </h3>
              <p className="card-desc">
                {pickBi(
                  locale,
                  'Pour interdire les cascades de délégations infinies et les attaques par déni de service, le nombre de blocs d’atténuation est borné à 4 (5 blocs au total). Tout jeton signé par une clé racine non épinglée est rejeté en O(1).',
                  'To prevent delegation cascade loops and DoS complexity attacks, delegation depth is strictly capped at D_max = 4 (5 blocks total). Self-signed tokens from untrusted root keys fail verification immediately.'
                )}
              </p>
              <div className="proof-metrics-list">
                <div className="proof-metric-item">
                  <span className="metric-label">{pickBi(locale, 'Profondeur maximale :', 'Max Delegation Depth:')}</span>
                  <strong className="mono metric-val">D_max = 4 (max 5 blocs)</strong>
                </div>
                <div className="proof-metric-item">
                  <span className="metric-label">{pickBi(locale, 'Temps de vérification :', 'Verifier Timeout:')}</span>
                  <strong className="mono metric-val">max_time &lt;= 500 ms</strong>
                </div>
                <div className="proof-metric-item">
                  <span className="metric-label">{pickBi(locale, 'Engagement Révocation :', 'Revocation Commitment:')}</span>
                  <strong className="mono metric-val">C = SHA-256(Token || Salt)</strong>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* DEEP DIVE SECTION 3: MULTI-CLIENT IPC PROTOCOL */}
        <section className="arch-deep-dive-section">
          <div className="section-header">
            <span className="eyebrow-mono">CRATES/HAKILLE-IPC & HAKILLE-DAEMON · PROTOCOLE FILAIRE</span>
            <h2 className="section-h2">
              {pickBi(
                locale,
                'Protocole IPC Multi-Clients & Isolation Sockets 0600',
                'Multi-Client IPC Protocol & 0600 Socket Isolation'
              )}
            </h2>
            <p className="section-lead">
              {pickBi(
                locale,
                'Le démon Hakille coordonne les shells natifs (Desktop HUD, Mobile Companion, CLI) via un protocole asynchrone non-bloquant sur sockets de domaine Unix et named pipes Windows.',
                'The Hakille daemon coordinates native client shells (Desktop HUD, Mobile Companion, CLI) via an asynchronous, non-blocking protocol over Unix domain sockets and Windows named pipes.'
              )}
            </p>
          </div>

          <div className="ipc-specs-grid">
            <div className="ipc-card">
              <span className="eyebrow-mono">{pickBi(locale, 'Cadrage Filaire & Sécurité', 'Wire Framing & Security')}</span>
              <ul className="clean-list" style={{ fontSize: '0.86rem', marginTop: '10px' }}>
                <li>◆ <strong>Plafond de trame :</strong> 1 MiB (1 048 576 octets) max par trame JSON délimitée par newline (\n).</li>
                <li>◆ <strong>Authentification Token :</strong> Jeton CSPRNG de 32 octets (64 hex) avec permissions 0600 strictes.</li>
                <li>◆ <strong>Points de terminaison :</strong> <code>$XDG_RUNTIME_DIR/hakille/ipc.sock</code> ou <code>\\.\pipe\hakille-daemon-v1</code>.</li>
              </ul>
            </div>

            <div className="ipc-card">
              <span className="eyebrow-mono">{pickBi(locale, 'Discipline de Routage Broadcast vs Unicast', 'Broadcast vs Unicast Routing Discipline')}</span>
              <ul className="clean-list" style={{ fontSize: '0.86rem', marginTop: '10px' }}>
                <li>◆ <strong>Bus de Broadcast (id = 0) :</strong> Diffuse les alertes système, PresenceState et toasts à tous les clients.</li>
                <li>◆ <strong>Routage Unicast (id = request.id) :</strong> Réponses directes aux requêtes du client émetteur.</li>
                <li>◆ <strong>Streaming TokenDelta :</strong> Tokens LLM incrémentaux en streaming basse latence avec finalisation HudReply.</li>
              </ul>
            </div>
          </div>
        </section>

        {/* DEEP DIVE SECTION 4: KALMAN MULTIMODAL CONFIDENCE & PERCLOS */}
        <section className="arch-deep-dive-section">
          <div className="section-header">
            <span className="eyebrow-mono">CRATES/HAKILLE-AUTH/SRC/MULTIMODAL.RS & DOCS/DETECTION_MULTIMODALE.MD</span>
            <h2 className="section-h2">
              {pickBi(
                locale,
                'Perception Multimodale, Filtre de Kalman & Décroissance Temporelle',
                'Multimodal Perception, Kalman Filter & Temporal Confidence Decay'
              )}
            </h2>
            <p className="section-lead">
              {pickBi(
                locale,
                'Hakille évalue en continu la confiance d’authentification de la session en fusionnant 4 modalités sensorielles indépendantes avec décroissance exponentielle fail-closed.',
                'Hakille continuously tracks session confidence by fusing 4 independent biometric modalities using a 1D Kalman filter with fail-closed exponential decay.'
              )}
            </p>
          </div>

          <div className="kalman-equations-grid">
            <div className="equation-card">
              <span className="eyebrow-mono">{pickBi(locale, 'Équation de Décroissance Temporelle', 'Temporal Confidence Decay Equation')}</span>
              <div className="math-display mono">
                τ(t) = τ₀ · exp(-λ · t) &emsp; où &emsp; λ = ln(2) / t₁/₂ &emsp; (t₁/₂ = 180 s)
              </div>
              <p style={{ fontSize: '0.8rem', color: 'var(--hakille-texte-2)', margin: '8px 0 0 0' }}>
                {pickBi(
                  locale,
                  'La confiance chute de moitié toutes les 3 minutes en l’absence de confirmation visuelle. Après 15 minutes (900 s), la session se verrouille automatiquement (fail-closed : τ = 0.0, P = 1.0).',
                  'Confidence halves every 3 minutes without fresh visual confirmation. After 15 minutes (900 s), the session fails closed (τ = 0.0, P = 1.0).'
                )}
              </p>
            </div>

            <div className="equation-card">
              <span className="eyebrow-mono">{pickBi(locale, 'Estimation Sténopé de la Stature sans LiDAR', 'Pinhole Stature Estimation without LiDAR')}</span>
              <div className="math-display mono">
                Z = (f_x · 0.063) / Δx_pupils &emsp; | &emsp; H_estimée = 1 / 0.52 · (((y_hip - y_crown) · Z) / f_y + Δh_head)
              </div>
              <p style={{ fontSize: '0.8rem', color: 'var(--hakille-texte-2)', margin: '8px 0 0 0' }}>
                {pickBi(
                  locale,
                  'Triangulation passive basée sur la constante anthropométrique universelle IPD (63 mm) et le ratio assis (52,0 % ± 1,2 %).',
                  'Passive triangulation based on the universal human interpupillary distance (63 mm) and seated ratio constant (52.0% ± 1.2%).'
                )}
              </p>
            </div>
          </div>

          <div className="sensor-modalities-strip">
            <div className="sensor-cell">
              <span className="eyebrow-mono">Visage (BioHash)</span>
              <strong className="mono">Bruit R = 0.04</strong>
              <span style={{ fontSize: '0.74rem', color: 'var(--hakille-texte-3)' }}>Poids 50 %</span>
            </div>
            <div className="sensor-cell">
              <span className="eyebrow-mono">Voix (CAM++ 192-d)</span>
              <strong className="mono">Bruit R = 0.08</strong>
              <span style={{ fontSize: '0.74rem', color: 'var(--hakille-texte-3)' }}>Poids 30 %</span>
            </div>
            <div className="sensor-cell">
              <span className="eyebrow-mono">Frappe Clavier</span>
              <strong className="mono">Bruit R = 0.15</strong>
              <span style={{ fontSize: '0.74rem', color: 'var(--hakille-texte-3)' }}>Poids 10 %</span>
            </div>
            <div className="sensor-cell">
              <span className="eyebrow-mono">Proximité BLE RSSI</span>
              <strong className="mono">Bruit R = 0.25</strong>
              <span style={{ fontSize: '0.74rem', color: 'var(--hakille-texte-3)' }}>Poids 10 %</span>
            </div>
          </div>
        </section>

        {/* SECTION 14 ADRS */}
        <section>
          <div className="section-header">
            <span className="eyebrow-mono">ADR-001 .. ADR-014 (/adr/*.md)</span>
            <h2 className="section-h2">
              {pickBi(
                locale,
                'Les 14 Décisions Architecturales Fondatrices (ADR)',
                'The 14 Foundational Architectural Decision Records (ADRs)'
              )}
            </h2>
          </div>
          <div className="crate-grid">
            {ALL_14_ADRS.map((adr) => (
              <article key={adr.id} className="crate-item">
                <span className="mono ltr-isolate" style={{ color: 'var(--hakille-lueur)', fontWeight: 600 }}>
                  {adr.id}
                </span>
                <strong style={{ color: 'var(--hakille-texte-1)' }}>
                  {pickBi(locale, adr.titleFr, adr.titleEn)}
                </strong>
                <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)' }}>
                  {pickBi(locale, adr.detailFr, adr.detailEn)}
                </p>
              </article>
            ))}
          </div>
        </section>

        {/* SECTION HARDWARE TIERS */}
        <section>
          <div className="section-header">
            <span className="eyebrow-mono">MANIFESTS/*.TOML</span>
            <h2 className="section-h2">
              {pickBi(
                locale,
                'Matrice des 3 Paliers Matériels & 6 Substrats OS',
                '3 Hardware Tiers & 6 OS Substrates'
              )}
            </h2>
          </div>
          <div className="tier-grid-3">
            {HARDWARE_TIERS.map((tier) => (
              <article key={tier.id} className="arch-card">
                <span className="status-tag">{tier.badge}</span>
                <h3 style={{ fontSize: 'var(--hakille-type-heading-size)', fontWeight: 600 }}>
                  {pickBi(locale, tier.nameFr, tier.nameEn)}
                </h3>
                <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)' }}>
                  <strong>{pickBi(locale, 'Matériel :', 'Hardware:')}</strong>{' '}
                  {pickBi(locale, tier.targetHardwareFr, tier.targetHardwareEn)}
                </p>
                <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)' }}>
                  <strong>{pickBi(locale, 'Isolation :', 'Isolation:')}</strong>{' '}
                  {pickBi(locale, tier.isolationLevelFr, tier.isolationLevelEn)}
                </p>
                <p className="mono" style={{ fontSize: '0.76rem', color: 'var(--hakille-lueur)' }}>
                  {tier.ramFootprint}
                </p>
              </article>
            ))}
          </div>
        </section>
      </div>
    </div>
  );
}
