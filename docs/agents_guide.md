# Guide de Développement d'Agents Sandboxés WASM

Ce guide explique comment créer, tester et déployer des agents autonomes isolés au sein de l'environnement d'exécution WASM d'Annora.

---

## 1. Principes d'Isolation des Agents

Dans Annora, aucun agent tiers ou compétence autonome ne s'exécute directement dans le processus hôte :
1. **Isolation WASM/WASI** : Les agents sont compilés en cibles `wasm32-wasip1` ou `wasm32-wasip2` et exécutés dans des instances isolées orchestrées par `annora-agents` (basé sur **Wasmtime**).
2. **Capabilities Explicites** : Un agent ne dispose d'aucun accès au système de fichiers, au réseau ou aux périphériques par défaut.
3. **Passeport Cryptographique** : Chaque agent détient un passeport signé Ed25519 attestant de l'identité pour laquelle il agit et de la délégation de pouvoirs reçue.

---

## 2. Structure d'un Agent Annora

Un agent Annora est un crate Rust autonome contenant un fichier de manifeste `agent.toml` et un point d'entrée :

```
agents/my-custom-agent/
├── agent.toml      # Métadonnées et déclaration des capabilities exigées
├── Cargo.toml      # Dépendances (std ou core sans accès système direct)
└── src/
    └── main.rs     # Logique de l'agent
```

### Le Manifeste `agent.toml`
```toml
id = "agent-calendar"
name = "Calendar Agent"
version = "0.1.0"
description = "Gestionnaire d'agenda et planification locale"

# Déclaration obligatoire des capacités exigées
required_capabilities = [
    "Filesystem"
]
```

---

## 3. Exemple d'Implémentation (`src/main.rs`)

```rust
use std::io::{self, Read, Write};

fn main() {
    // 1. Les agents communiquent via les flux standard typés (stdin/stdout)
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).expect("Failed to read input");

    // 2. Traitement de la commande passée par annora-kernel
    let response = format!("Agent calendar executed action on input: {}", input.trim());

    // 3. Émission de la réponse structurée
    io::stdout().write_all(response.as_bytes()).expect("Failed to write output");
}
```

---

## 4. Les Agents de Référence Inclus

Le monorepo Annora fournit cinq agents de référence prêts à l'emploi dans `agents/` :

| Agent | Scopes de Capabilities | Rôle & Fonctionnalité |
| :--- | :--- | :--- |
| **`agent-mail`** | `Network`, `Filesystem` | Analyse des boîtes de réception, tri, proposition de réponses. Tout contenu externe est marqué `Untrusted`. |
| **`agent-calendar`** | `Filesystem` | Consultation et mise à jour des plannings sans accès réseau extérieur. |
| **`agent-code`** | `Filesystem` | Inspection de syntaxe, refactoring et exécution de tests dans un répertoire isolé. |
| **`agent-home`** | `Network`, `Sensor` | Communication avec les ponts domotiques locaux (Matter / Zigbee / Home Assistant). |
| **`agent-wallet-readonly`** | `Wallet` | Consultation des plafonds de dépenses alloués et vérification de conformité financière. |

---

## 5. Compilation et Enregistrement dans le Démon

Pour compiler un agent en bytecode WebAssembly :
```bash
cargo build -p agent-calendar --target wasm32-wasip1 --release
```

Le démon `annora-daemon` charge automatiquement les binaires compilés et valide les manifestes avant d'accorder les tokens Biscuit correspondants.
