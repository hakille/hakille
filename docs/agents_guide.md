# Agents de Référence et Runtime WASM

Ce guide décrit l'état **réel** des agents dans Hakille : ce qui existe, ce qui
est testé et ce qui n'est pas encore branché.

> **Statut v1 — important.** Le crate `hakille-agents` contient un runtime
> Wasmtime fonctionnel et testé. Depuis cette version, le démon **matérialise et
> exécute réellement deux agents de référence** (`echo-agent` et `count-agent`)
> à partir de sources WAT embarquées. Aucun binaire `.wasm` n'est versionné
> dans le dépôt : les artefacts sont compilés au démarrage, écrits en
> content-addressed sous `<data_dir>/agents`, vérifiés par SHA-256 puis
> auto-testés.
>
> Les cinq dossiers de `agents/` restent des **crates natives avec manifests
> déclaratifs** (`agent-mail`, `agent-calendar`, `agent-code`, `agent-home`,
> `agent-wallet-readonly`). Aucun binaire métier n'est livré ou exécuté, et
> `spawn_sandboxed` ne lance pas encore ces crates. Les scopes déclarés dans
> leurs manifests ne sont donc **pas encore appliqués** par un bac à sable en
> exécution.

---

## 1. Ce qui existe réellement

| Élément | État |
| :--- | :--- |
| `AgentSandbox` / `Agent::run` (Wasmtime) | Implémenté et testé : ABI `run`, zéro import, fuel, timeout, mémoire bornée. |
| ABI agent `memory` + `run(i32,i32)->(i32,i32)` | Implémentée ; entrée JSON UTF-8 écrite en mémoire, sortie JSON validée. |
| Agents de référence `echo-agent`, `count-agent` | Sources WAT embarquées, compilées en WASM, matérialisées et exécutées par le démon. |
| Artefacts WASM sur disque | Générés sous `<data_dir>/agents/<nom>-<sha256>.wasm` ; hash vérifié avant chargement. |
| Auto-test au démarrage | Exécuté sur les deux agents de référence (fuel 10M, timeout 2 s, 32 pages). |
| Commande `:agents-run <nom> <json>` | Exécute réellement un agent de référence, avec audit avant/après. |
| 5 crates `agents/*` | Fonctions natives et tests unitaires présents. |
| 5 manifests `agent.toml` | Présents ; enregistrés en mémoire par le démon comme **déclaratifs**. |
| Chargement dynamique des manifests par le démon | Absent : la liste métier est codée en dur. |
| Binaires `.wasm` métier | **Absents du dépôt et non exécutés**. |
| Exécution d'un agent métier par le démon | **Absente** : `spawn_sandboxed` n'appelle pas le runtime pour ces crates. |
| Application effective des scopes Biscuit aux agents | **Absente** : les scopes sont des déclarations, pas une autorisation vérifiée. |

Le REPL `:agents` distingue donc explicitement :

- les **cinq manifests métier déclaratifs**, sans binaire ;
- le **runtime WASM de référence réel**, avec chemins, hashes et résultat de
  l'auto-test.

---

## 2. Structure d'un agent

Un dossier d'agent contient :

```
agents/my-agent/
├── agent.toml      # métadonnées + capabilities déclarées
├── Cargo.toml
└── src/
    ├── lib.rs      # logique native testable
    └── main.rs     # point d'entrée natif (pas l'ABI du runtime WASM)
```

### Manifeste `agent.toml`

```toml
id = "agent-calendar"
name = "Calendar Agent"
version = "0.1.0"
description = "Planification locale"
required_capabilities = ["Filesystem"]
isolated = true
```

`required_capabilities` est une **déclaration**. Elle ne crée ni ne vérifie de
jeton Biscuit à elle seule.

Le manifeste Rust correspondant est :

```rust
use hakille_agents::AgentManifest;
use hakille_core::CapabilityScope;

let manifest = AgentManifest {
    id: "agent-mail".to_string(),
    name: "MailAssistant".to_string(),
    version: "0.1.0".to_string(),
    description: "Lecture et organisation des emails".to_string(),
    required_capabilities: vec![CapabilityScope::Filesystem, CapabilityScope::Network],
};
```

---

## 3. ABI du runtime WASM

Un module destiné au runner doit exporter exactement :

- `memory` : mémoire linéaire ;
- `run : (i32, i32) -> (i32, i32)` : `run(input_ptr, input_len)` retourne
  `(output_ptr, output_len)`.

Contraintes réellement appliquées par `AgentSandbox` :

1. **Aucun import** n'est lié (ni WASI, ni `env`) ; un module qui importe quoi
   que ce soit est refusé au chargement.
2. L'entrée et la sortie sont du **JSON UTF-8 valide**.
3. Chaque appel utilise un `Store` neuf : aucun état résiduel entre deux runs.
4. Quotas par défaut : `fuel = 50_000_000`, `timeout = 5 s`,
   `max_memory_pages = 256` (16 Mio), surchargeables via `RunLimits`.
5. Le runtime du démon applique des limites plus strictes à chaque exécution :
   `fuel = 10_000_000`, `timeout = 2 s`, `max_memory_pages = 32` (2 Mio), et
   refuse une entrée de plus de 64 Kio.

Ce contrat est testé dans `crates/hakille-agents/src/wasm_runner.rs` et
`crates/hakille-agents/src/reference.rs`. Les cinq agents métier ne respectent
pas encore cet ABI : leurs `main.rs` utilisent stdin/stdout natif.

---

## 4. Artefacts de référence et exécution

Les sources WAT de `echo-agent` et `count-agent` vivent dans
`wasm_runner.rs`. Au démarrage du démon :

1. `reference_agents_wasm()` compile les WAT avec le crate `wat` ;
2. `materialize_reference_agents()` écrit chaque binaire sous
   `<data_dir>/agents/<nom>-<sha256>.wasm` (écriture temp + rename) ;
3. le hash SHA-256 du fichier est relu et vérifié ; un fichier altéré est
   refusé (`ArtifactHashMismatch`) et jamais exécuté ;
4. `ReferenceAgentRuntime::provision()` charge le binaire dans
   `AgentSandbox`, exécute un auto-test JSON et conserve l'agent en mémoire.

Commandes utiles :

```text
:agents
:agents-run echo-agent {"msg":"bonjour"}
:agents-run count-agent {"msg":"bonjour"}
```

Chaque exécution réelle écrit deux entrées d'audit : demande
(`WasmAgentRunRequested`) puis résultat (`WasmAgentRunCompleted` ou
`WasmAgentRunFailed`). Les payloads ne sont pas journalisés en clair ; seuls
les hashes SHA-256, la durée et la taille de sortie le sont.

---

## 5. Compiler un module compatible (mode manuel)

Le chemin de compilation d'un agent métier vers WASM existe, mais il n'est pas
automatisé par le démon :

```bash
rustup target add wasm32-wasip1
cargo build -p mon-agent --target wasm32-wasip1 --release
```

Le module produit doit être chargé explicitement par un hôte qui appelle
`AgentSandbox::load` puis `Agent::run` avec des `RunLimits`. Le démon ne le fait
aujourd'hui que pour les deux agents de référence embarqués, pas pour les cinq
crates métier.

---

## 6. Travaux nécessaires pour les agents métier

1. Exporter l'ABI `run`/`memory` depuis les cinq agents métier (ou fournir des
   adaptateurs WASM).
2. Produire des binaires reproductibles et les déclarer dans un catalogue
   content-addressed (hash + version).
3. Brancher `spawn_sandboxed` sur le runtime pour les agents métier, avec
   sélection par manifeste.
4. Dériver les capabilities Biscuit depuis le manifeste et les vérifier avant
   chaque run ; aujourd'hui les scopes ne sont que déclarés.
5. Ajouter un test d'intégration démon → runner → réponse JSON pour chaque
   agent métier, puis mettre à jour ce guide et `README.md`.

Tant que ces étapes ne sont pas faites, toute documentation ou sortie qui
présente les cinq agents métier comme « sandboxés et opérationnels » est
incorrecte. Les deux agents de référence, eux, sont réellement compilés,
vérifiés et exécutés.
