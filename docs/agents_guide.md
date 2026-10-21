# Agents de Référence et Runtime WASM

Ce guide décrit l'état **réel** des agents dans Hakille : ce qui existe, ce qui
est testé et ce qui n'est pas encore branché.

> **Statut v1 — important.** Le crate `hakille-agents` contient un `WasmRunner`
> Wasmtime fonctionnel et testé, mais le démon ne l'utilise pas encore. Les cinq
> dossiers de `agents/` sont des **crates natives avec manifests** ; aucun
> binaire `.wasm` n'est versionné, chargé ou exécuté. `spawn_sandboxed` retourne
> un handle local sans lancer d'instance WASM. Les scopes déclarés dans les
> manifests ne sont donc **pas encore appliqués** par un bac à sable en
> exécution.

---

## 1. Ce qui existe réellement

| Élément | État |
| :--- | :--- |
| `WasmRunner` / `Agent::run` (Wasmtime) | Implémenté et testé unitairement : ABI `run`, zéro import, fuel, timeout, mémoire bornée. |
| ABI agent `memory` + `run(i32,i32)->(i32,i32)` | Implémentée ; entrée JSON UTF-8 écrite en mémoire, sortie JSON validée. |
| 5 crates `agents/*` | Fonctions natives et tests unitaires présents. |
| 5 manifests `agent.toml` | Présents ; enregistrés en mémoire par le démon. |
| Chargement dynamique des manifests par le démon | Absent : la liste est codée en dur. |
| Binaires `.wasm` | **Absents du dépôt**. |
| Exécution d'un agent par le démon | **Absente** : `spawn_sandboxed` n'appelle pas `WasmRunner`. |
| Application effective des scopes Biscuit aux agents | **Absente** tant que l'exécution n'est pas branchée. |

Le REPL `:agents` affiche donc des manifests et un avertissement explicite,
jamais un agent « en cours d'exécution ».

---

## 2. Structure d'un agent

Un dossier d'agent contient :

```
agents/my-agent/
├── agent.toml      # métadonnées + capabilities déclarées
├── Cargo.toml
└── src/
    ├── lib.rs      # logique native testable
    └── main.rs     # point d'entrée natif (pas l'ABI WasmRunner)
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

## 3. ABI du `WasmRunner` (cible d'exécution)

Un module destiné au runner doit exporter exactement :

- `memory` : mémoire linéaire ;
- `run : (i32, i32) -> (i32, i32)` : `run(input_ptr, input_len)` retourne
  `(output_ptr, output_len)`.

Contraintes réellement appliquées par `WasmRunner` :

1. **Aucun import** n'est lié (ni WASI, ni `env`) ; un module qui importe quoi
   que ce soit est refusé au chargement.
2. L'entrée et la sortie sont du **JSON UTF-8 valide**.
3. Chaque appel utilise un `Store` neuf : aucun état résiduel entre deux runs.
4. Quotas par défaut : `fuel = 50_000_000`, `timeout = 5 s`,
   `max_memory_pages = 256` (16 Mio), surchargeables via `RunLimits`.

Ce contrat est testé dans `crates/hakille-agents/src/wasm_runner.rs`. Il ne
signifie pas que les cinq agents de référence respectent déjà cet ABI : leurs
`main.rs` utilisent stdin/stdout natif.

---

## 4. Compiler un module compatible (mode manuel)

Le chemin de compilation existe, mais il n'est pas automatisé par le démon :

```bash
rustup target add wasm32-wasip1
cargo build -p mon-agent --target wasm32-wasip1 --release
```

Le module produit doit être chargé explicitement par un hôte qui appelle
`AgentSandbox::load` puis `Agent::run` avec des `RunLimits`. Le démon actuel ne
le fait pas.

---

## 5. Travaux nécessaires pour l'exécution de bout en bout

1. Exporter l'ABI `run`/`memory` depuis les agents (ou fournir des adaptateurs).
2. Fournir et versionner des binaires `.wasm` reproductibles.
3. Brancher `spawn_sandboxed` sur `WasmRunner` au lieu du handle local.
4. Dériver les capabilities Biscuit depuis le manifeste et les vérifier avant
   chaque run.
5. Ajouter un test d'intégration démon → runner → réponse JSON, puis mettre à
   jour ce guide et `README.md`.

Tant que ces étapes ne sont pas faites, toute documentation ou sortie qui
présente les agents comme « sandboxés et opérationnels » est incorrecte.
