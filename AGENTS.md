# AGENTS.md — Hakille

Monorepo Rust (31 crates + 5 crates d'agents + `conformance` + `xtask`), édition 2024,
MSRV/toolchain **1.95** (épinglé par `rust-toolchain.toml`).
Contrat produit central : **aucun succès simulé** (voir « Contrat d'honnêteté »).

## Commandes (ordre du gate CI)

`cargo xtask` est un alias (`.cargo/config.toml`) → `cargo run --package xtask --`.

```bash
cargo fmt --all -- --check
cargo run -p xtask -- tokens --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --no-deps
cargo test --workspace --locked
cargo run -p xtask -- conformance          # = cargo test -p conformance (66 fast)
cargo deny check bans sources advisories    # cargo-deny à installer
```

- `cargo xtask conformance-slow` : gate hebdo (**12 tests `--ignored`** — 7 dans `conformance`, 5 dans `hakille-kernel` — décodage Qwen CPU réel, dizaines de minutes en debug). Ne pas le lancer en local en debug. Ces tests **ne SKIP pas** sans modèle : ils échouent (c'est voulu).
- Provisionner le modèle pour ce gate : `cargo run -p hakille-daemon -- models download --model qwen2.5-0.5b-instruct-q4km` (~475 Mo) puis `models verify` (les deux commandes **échouent** si le téléchargement ou le SHA-256 échoue).
- Test ciblé : les tests unitaires vivent **dans les crates** (`#[cfg(test)] mod tests`) ; les crates d'agents `agents/*` ont en plus `tests/integration.rs`. Filtres utiles : `cargo test -p hakille-ui-desktop card_`, `cargo test -p hakille-daemon card_`, `cargo test -p hakille-kernel action_card_preview_tests`.
- Le job CI `ui-desktop-quality` ne lance que des **filtres par nom** (`a11y`, `renderer`, `wgpu_fallback`, `s2`, `desktop_shell`) : un nouveau test n'y passe que si son nom contient le filtre.
- `cargo clippy` en CI **n'a pas** `-D warnings` (choix assumé, ~60 lints préexistants) et il n'y a **pas** de `[workspace.lints]` : ne pas en ajouter un, corriger les lints dans le crate touché.
- `Cargo.lock` est versionné : valider avec `--locked`.

## Pièges non évidents

- **Le profil debug compile déjà candle optimisé** (`[profile.dev.package.candle-*]` + `tokenizers` en `opt-level = 3` dans le `Cargo.toml` racine) : un tour Qwen CPU passe de ~478 s à ~25 s de décodage. Le release reste plus rapide (~24 s pour 64 tokens, mesuré) et le 1er tour ajoute le chargement lazy du GGUF (~10-60 s).
- **Un timeout n'arrête pas le décodage.** `crates/hakille-kernel/src/system1.rs` enveloppe `spawn_blocking` dans `tokio::time::timeout` : à l'expiration, la tâche bloquante **continue** et garde le verrou global du moteur, donc la requête suivante attend derrière. Ne pas interpréter « timeout » comme « CPU arrêté » (corrigé depuis : le moteur reçoit une **échéance dure**, `generate_with_deadline`).
- Le budget pipeline est **dérivé** du timeout LLM (`HAKILLE_QWEN_TIMEOUT_MS`, défaut 600 000 ms) : `max_inference = timeout + 30 s`, `max_total = timeout + 60 s` (`crates/hakille-kernel/src/budget.rs`). Changer le timeout change le budget du boot.
- `models/` est **gitignoré** (~2,36 Go) : les tests qui exigent un artefact doivent `SKIP: artefact de modèle absent` ou échouer explicitement. Ne jamais commiter `models/`.
- `crates/hakille-design/src/generated.rs` est **généré** : la source unique est `tokens/*.toml`. Modifier les tokens → `cargo xtask tokens` (regénère le fichier) ; ne jamais éditer `generated.rs` à la main.
- `crates/hakille-ui-protocol/src/schema_snapshot.json` est un **golden** : tout changement de message du protocole fait échouer `test_schema_snapshot_stable`, qui exige de mettre à jour le golden **et** `PROTOCOL_VERSION`.
- Le fond noir de la fenêtre desktop est une **limite assumée** : softbuffer 0.4 ne compose pas l'alpha, donc `transparent=false` est forcé dans `bin/desktop.rs` (et `Present` ignore l'alpha). Ce n'est pas un bug de rendu.
- Accents : PowerShell affiche le contenu UTF-8 en mojibake (artefact de console, pas de fichier corrompu). Ne pas « corriger » l'encodage des fichiers.

## Architecture (points d'entrée réels)

- `crates/hakille-daemon/src/main.rs` : binaire unique (REPL `hakille>` avec commandes `:status`, `:models`, `:agents-run …`; `--serve` = mode IPC; `--eval-prompt` = un tour headless). Le REPL est résilient : un tour échoué n'arrête pas la session.
- `crates/hakille-ui-desktop/` : `src/app.rs` = machine à états **pure** (`HudApp` : `HudInput` → `HudAction`, aucun effet de bord) ; `src/renderer.rs` = scènes déterministes ; `src/bin/desktop.rs` = hôte winit ; `src/ipc_client.rs` = client IPC dans un thread tokio dédié (les réponses reviennent au thread winit via `EventLoopProxy`).
  - Le HUD ne simule **jamais** un effet : l'hôte ne fait qu'exécuter les `HudAction` (soumettre, redimensionner 720×64 / 720×360, requêtes journal/mémoire/statut).
- Transport IPC : named pipe Windows `\\.\pipe\hakille-daemon-v1` / socket Unix `$XDG_RUNTIME_DIR/hakille/ipc.sock` (repli `/tmp`) ; jeton dans `%LOCALAPPDATA%\Hakille\ipc.token` (`$XDG_RUNTIME_DIR/hakille/ipc.token` hors Windows), jamais journalisé ; overrides `HAKILLE_IPC_ENDPOINT`, `HAKILLE_IPC_TOKEN_FILE`.
- Données locales : `%LOCALAPPDATA%\Hakille\` → `keys/`, `audit.redb` (journal append-only Ed25519 chaîné), `memory.json`, `outbox.json`, `agents/`. Pour un test à blanc : supprimer ce dossier.
- Agents : la source WAT des agents de référence est **embarquée** dans `crates/hakille-agents/src/wasm_runner.rs`, compilée à l'exécution par le crate `wat` et matérialisée content-addressed (sha256) dans `<data_dir>/agents` (un artefact corrompu doit être refusé, test existant). Les 5 crates `agents/*` sont **déclaratifs** : leur `agent.toml` n'est pas chargé par le démon, aucun `.wasm` métier n'est livré, `spawn_sandboxed` n'est pas branché. Ne pas les présenter comme exécutés.
- Contrat agents IPC (additif v1) : `UiToKernel::AgentListRequest` / `AgentRunRequest` → `KernelToUi::AgentListReply` / `AgentRunReply`. `AgentDto.artifact_present` distingue l'agent réellement exécutable du manifest déclaratif ; une liste de Capability n'est jamais inventée. Le runtime est `Arc<Mutex<ReferenceAgentRuntime>>` (l'exécution Wasmtime est bloquante → `spawn_blocking`). Cote HUD : `F4` ouvre le panneau, `:agent <nom> <json>` exécute un agent.
- Cartes d'action : le démon propose `ShowCard` → le desktop décide (Entrée **maintenue 600 ms** approuve, `R` refuse, `Échap` masque ; nœuds AccessKit 40/41/42 ; les 3 boutons sont cliquables à la souris, la souris respecte le maintien 600 ms) → le démon **re-vérifie et audite** avant `CardDecisionAck` (UUID d'audit réel, champ `effect` = effet réellement produit).
- Effet d'une carte approuvée : l'action est **mise en file dans l'outbox durable** (`<data_dir>/outbox.json`, idempotent, revalidable) et l'audit enregistre `effect: "outbox_enqueued"`. Ce n'est **pas** une exécution métier (les agents métier ne sont pas exécutables) : ne jamais dire « exécuté ».
- Le desktop : fond transparent réel sous Windows (fenêtre en couche Win32 + clé couleur `0x010101`) ; la fenêtre est déplaçable (barre du HUD / en-tête de panneau) ; pendant un tour, l'attente affiche le temps réel écoulé et la saisie est verrouillée (pas de second tour concurrent).
- Le `timeout` de `system1.rs` est doublé d'une **échéance dure dans le moteur** (`generate_with_deadline`) : à l'échéance, le décodage s'arrête et libère le verrou (plus de génération « zombie » qui bloque les requêtes suivantes).

## Contrat d'honnêteté (non négociable)

- Pas de stub silencieux, de mesure inventée, de « fallback » qui masque un échec : artefact/détecteur/permission/binding absent → erreur explicite ou `SKIP` libellé.
- Ne pas affaiblir les capabilities Biscuit, les frontières de sandbox, la chaîne d'audit.
- Ne jamais commiter `models/`, secrets, jetons, clés privées, données personnelles (`ipc.token` en particulier).
- Toute limite non levée doit rester **écrite** (README, `docs/`, `SECURITY.md`) plutôt que masquée. Le README et `docs/DEVELOPER_GUIDE.md` listent l'état réel par moteur (`is_simulated()`, `:models-honest`).

## Conventions

- Commentaires et messages utilisateur **en français** ; identifiants en anglais. Formatage : `cargo fmt` (défaut rustfmt).
- PR : changements ciblés, tests ajoutés/mis à jour, docs mises à jour si un contrat change. `main` est protégé : merge **squash**, historique linéaire, tous les jobs requis verts.
- `deny.toml` documente les advisories acceptées en connaissance de cause : ne pas les retirer sans vérifier.
- Sources d'instructions à lire avant d'agir : `README.md` (statut réel), `CONTRIBUTING.md` (contrat d'honnêteté + gates), `SECURITY.md`, `docs/DEVELOPER_GUIDE.md`, `docs/ARCHITECTURE.md`, `adr/`.
