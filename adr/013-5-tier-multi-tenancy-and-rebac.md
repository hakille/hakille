# ADR 013: Hiérarchie Multi-Tenant à 5 Niveaux, Moteur d'Accès Hybride (ReBAC + Biscuit) et Délégation Souveraine d'Agents

## Statut
Accepté (Mis à jour pour la version souveraine de Hakille)

## Contexte
Hakille est un système d'exploitation cognitif destiné à fonctionner dans une pluralité d'environnements hétérogènes :
- **Sphère Domestique et Familiale** : Périphériques partagés entre adultes, enfants et invités temporaires, imposant une protection absolue des mineurs et un cloisonnement strict des finances.
- **Espaces Collaboratifs d'Agents Autonomes** : Essaims d'agents logiciels exécutant des flux cognitifs en micro-tâches coordonnées, nécessitant des délégations fines d'autorité sans transmission de clés maîtresses.
- **Structures Corporatives et Conglomérats Multi-Filiales** : Organisations formelles avec officiers de conformité, annuaires d'entreprise (SCIM), et pare-feu inter-entités impénétrables.

Les modèles d'accès traditionnels échouent dans ce contexte :
1. **Les ACLs plates et le RBAC statique** ne peuvent pas modéliser les relations dynamiques de dépendance (e.g. "le tuteur légal de Bob peut autoriser un temps d'écran supplémentaire") ni empêcher la prolifération incontrôlée de rôles globaux.
2. **L'authentification ponctuelle (Login unique)** est aveugle aux variations de contexte physique (éloignement de l'utilisateur, changement d'interlocuteur face à la caméra).
3. **La délégation d'agents non bornée** expose au risque catastrophique d'escalade de privilèges (*Confused Deputy Problem*) où un sous-agent de confiance compromise outrepasse les limites de son créateur.

---

## Architecture de la Tenancy et du Moteur d'Autorisation Hybride

```
+----------------------------------------------------------------------------------------------------+
|                         HIÉRARCHIE MULTI-TENANT À 5 NIVEAUX CARDINAUX                              |
|                                                                                                    |
|   +--------------------------------------------------------------------------------------------+   |
|   | ENTERPRISE (Conglomérats, Multi-filiales, Racine HSM/KMS, Isolation Inter-Org Souveraine)  |   |
|   +--------------------------------------------------------------------------------------------+   |
|         |                                                                                          |
|         v                                                                                          |
|   +---------------------------------------+          +-------------------------------------+       |
|   | ORGANIZATION (Département, ONG, SCIM) |          | GROUP (Cercle Ad-hoc, Quorum Join)  |       |
|   +---------------------------------------+          +-------------------------------------+       |
|         |                                                                                          |
|         v                                                                                          |
|   +---------------------------------------+          +-------------------------------------+       |
|   | TEAM (Workspace Agentique Partagé)    |          | FAMILY (Sphère Domestique, Règle 7) |       |
|   +---------------------------------------+          +-------------------------------------+       |
|                                                              |                                     |
|                                                              +--> Mineurs: Invariant NoWallet      |
|                                                              +--> Invités: Sessions bornées <= 24h |
+----------------------------------------------------------------------------------------------------+
                                               |
                                               v
+----------------------------------------------------------------------------------------------------+
|                               MOTEUR D'AUTORISATION HYBRIDE HAKILLE                                |
|                                                                                                    |
|   Requête d'Accès : (Sujet, Action, Ressource, Contexte, Jeton de Capacité)                        |
|                                               |                                                    |
|   [ ÉTAPE 1 : PARE-FEU INTER-TENANTS ] -------+                                                    |
|   Vérification stricte de séparation : Les tenants "Family" sont hermétiquement cloisonnés.       |
|   Tout pontage inter-organisations requiert un consentement bilatéral cryptographique.             |
|                                               | (Succès)                                           |
|                                               v                                                    |
|   [ ÉTAPE 2 : RÉSOLUTION REBAC ZANZIBAR ] ----+                                                    |
|   Résolution récursive de graphe : <entity>#<relation>@<subject> (DFS avec détection de cycles).   |
|   Relations directes, héritage hiérarchique, et appartenance transitive d'équipes.                 |
|                                               | (Succès)                                           |
|                                               v                                                    |
|   [ ÉTAPE 3 : CONTRÔLE STATIQUE RBAC ] -------+                                                    |
|   Vérification du rôle socle de l'utilisateur (Owner, Admin, Member, Guest).                       |
|                                               | (Succès)                                           |
|                                               v                                                    |
|   [ ÉTAPE 4 : CONFIANCE CONTINUE ABAC (tau) ] +                                                    |
|   Score de confiance multimodale tau in [0.0, 1.0] et niveau matériel (Tier 1 Enclave, Tier 2, 3).  |
|   Exigence tau >= 0.90 pour Wallet & Administration Système.                                       |
|                                               | (Succès)                                           |
|                                               v                                                    |
|   [ ÉTAPE 5 : VALIDATION BISCUIT DATALOG ] ---+                                                    |
|   Vérification cryptographique du jeton Biscuit v5 : exécution des règles et des caveats Datalog.  |
|   Vérification de la chaîne d'atténuation monotone hors-ligne (Profondeur D <= 4).                 |
|                                               |                                                    |
|                                               v                                                    |
|                                 DÉCISION FINALE : ALLOW / DENY                                     |
+----------------------------------------------------------------------------------------------------+
```

---

## Chaîne de Délégation Agentique Souveraine ($D_{\text{max}} = 4$)

Les agents autonomes créés dans Hakille reçoivent une identité souveraine décentralisée propre (`did:key:z...`) et un passeport d'agent cryptographique signé :

```
+----------------------------------------------------------------------------------------------------+
|                                CHAÎNE DE DÉLÉGATION D'AGENTS BORNÉE                                |
|                                                                                                    |
|   +-----------------------+                                                                        |
|   | Utilisateur Principal | (Niveau D = 0, Pleine Autorité, DID Principal)                        |
|   +-----------------------+                                                                        |
|               |                                                                                    |
|               | Émet un AgentPassport signé + Jeton Biscuit Racine                                 |
|               v                                                                                    |
|   +-----------------------+                                                                        |
|   | Agent 1 (Orchestrateur| (Niveau D = 1, did:key:z1..., Budget: 500c, Exp: 1h)                  |
|   +-----------------------+                                                                        |
|               |                                                                                    |
|               | Atténuation Datalog hors-ligne (Monotone : Budget <= 200c, Scope restreint)        |
|               v                                                                                    |
|   +-----------------------+                                                                        |
|   | Agent 2 (Chercheur)   | (Niveau D = 2, did:key:z2..., Budget: 200c, Exp: 30m)                 |
|   +-----------------------+                                                                        |
|               |                                                                                    |
|               | Atténuation Datalog hors-ligne (Budget <= 50c, Read-only sur table mémoire)        |
|               v                                                                                    |
|   +-----------------------+                                                                        |
|   | Agent 3 (Indexeur)    | (Niveau D = 3, did:key:z3..., Budget: 50c, Exp: 10m)                  |
|   +-----------------------+                                                                        |
|               |                                                                                    |
|               | Atténuation Datalog finale (Périmètre micro-noyau unique)                          |
|               v                                                                                    |
|   +-----------------------+                                                                        |
|   | Agent 4 (Sous-tâche)  | (Niveau D = 4 = D_max, did:key:z4..., Budget: 10c, Exp: 2m)            |
|   +-----------------------+                                                                        |
|               |                                                                                    |
|               X  TENTATIVE DE DÉLÉGATION D = 5 : REJET STRICT (Invariant Règle 4 / MaxDepthExceeded)|
+----------------------------------------------------------------------------------------------------+
```

---

## Formulations Mathématiques et Logiques

### 1. Algèbre ReBAC Style Google Zanzibar

Le graphe d'autorisations relationnelles est défini comme un ensemble de 5-tuples discrets :

$$\mathcal{T} = \{ (e_{\text{type}}, e_{\text{id}}, r, s_{\text{type}}, s_{\text{id}}) \} \subset \Sigma^* \times \Sigma^* \times \Sigma^* \times \Sigma^* \times \Sigma^*$$

Syntaxe canonique : `entity_type:entity_id#relation@subject_type:subject_id`

Soit l'opérateur d'évaluation relationnelle $\text{HasRelation}(e, r, s)$ calculé sur la fermeture transitive $R^+$ du graphe d'accessibilité :

$$\text{HasRelation}(e, r, s) \iff (e, r, s) \in \mathcal{T} \lor \exists u \in \mathcal{U} : \left( (e, r, u) \in \mathcal{T} \land \text{HasRelation}(u, r, s) \right)$$

#### Détection Statique de Cycles
Pour interdire les dénis de service par récursion infinie dans les relations mutuelles (e.g. $A \text{ member of } B \land B \text{ member of } A$), la résolution utilise un parcours en profondeur (DFS) avec ensemble de chemin actif $\mathcal{P} \subset \Sigma^*$ :

$$\text{Invariant} : \forall v \in \text{Voisins}(u), \; v \notin \mathcal{P} \implies \mathcal{P}' = \mathcal{P} \cup \{u\}$$
Si $v \in \mathcal{P}$, l'évaluation s'interrompt immédiatement avec l'émission de `Decision::Deny("Cycle relationnel détecté")`.

### 2. Évaluation Confinée Biscuit v5 en Logique Datalog

Les jetons Biscuit v5 encapsulent des faits, des règles de dérivation et des vérifications (*caveats*).
Soit un jeton $B$ composé de blocs $B_0, B_1, \dots, B_k$.

- **Bloc racine $B_0$** signé par la clé privée de l'autorité :
  $$B_0 = \{ \text{facts}, \text{rules}, \text{caveats} \}_{\text{Ed25519}(K_{\text{auth}})}$$
- **Atténuation par blocs successifs $B_{j}$ ($1 \le j \le k$)** :
  Chaque bloc subséquent ajoute des restrictions $\Delta C_j$ sans pouvoir modifier ou révoquer les faits de $B_0$.
  L'ensemble des mondes admissibles satisfaisant les caveats $\mathcal{C}$ vérifie la propriété de monotonie stricte :
  $$\text{Modèles}(B_{j}) \subseteq \text{Modèles}(B_{j-1}) \quad \forall j \ge 1$$
  Il est ainsi mathématiquement impossible d'élargir les permissions d'un jeton reçu.

### 3. Fonction d'Évaluation Hybride Contextuelle

L'octroi d'une permission pour une action $a \in \mathcal{A}$ sur une ressource $x$ dans un contexte $\mathcal{K} = (\tau, \text{Tier}_{\text{dev}}, \text{Tenant})$ est régi par la fonction indicatrice booléenne :

$$\Phi(s, a, x, \mathcal{K}) = \text{Firewall}(\text{Tenant}_s, \text{Tenant}_x) \land \text{ReBAC}(s, r_a, x) \land \text{RBAC}(\text{Role}_s, a) \land \text{ABAC}(\tau, \text{Tier}_{\text{dev}}, a) \land \text{BiscuitVerify}(B, s, a, x)$$

avec le prédicat ABAC de niveau de confiance continue :

$$\text{ABAC}(\tau, \text{Tier}, a) = \begin{cases}
\text{True} & \text{si } a \notin \text{CriticalActions} \land \tau \ge \tau_{\text{base}} \\
\text{True} & \text{si } a \in \text{CriticalActions} \land \tau \ge 0.90 \land \text{Tier} = \text{Tier1HardwareEnclave} \\
\text{False} & \text{sinon}
\end{cases}$$

### 4. Invariants des 5 Tiers et Protection des Mineurs

1. **Règle 7 (Protection Inviolable des Mineurs)** :
   Pour tout utilisateur $u$ appartenant à un tenant de type `Family` et marqué comme `MinorProfile` :
   $$\forall a \in \{\text{WalletTransfer}, \text{ContractSignature}, \text{CredentialExport}\}, \quad \Phi(u, a, \cdot, \cdot) = \text{False}$$
   Aucune règle ReBAC, aucun rôle RBAC, et aucune atténuation de jeton ne peut surcharger cette interdiction codée en dur dans le type system.
2. **Sessions Invités (*Guest Sessions*)** :
   Pour toute session invité $g$, la durée d'expiration $\Delta t$ vérifie :
   $$0 < \Delta t \le 1440\,\text{minutes (24 heures)}$$
   $$\text{Scope}(g) \cap \{\text{CapabilityScope::Wallet}, \text{CapabilityScope::FullSystem}\} = \emptyset$$

---

## Modèle de Menace (STRIDE & Confused Deputy)

| Catégorie STRIDE | Vecteur d'Attaque Spécifique | Mesure de Défense & Invariant Hakille |
| :--- | :--- | :--- |
| **Spoofing** | Agent usurpant l'identité d'un autre agent pour réclamer des ressources. | Chaque agent génère sa propre paire de clés Ed25519 (`did:key`) ; les requêtes sont signées numériquement et validées par le passeport d'agent. |
| **Tampering** | Modification d'un bloc de jeton Biscuit en vol pour supprimer un caveat de budget. | Tout jeton atténué lie cryptographiquement les blocs précédents via des clés publiques éphémères ; la falsification d'un bloc invalide toute la chaîne de signatures. |
| **Repudiation** | Un agent principal conteste avoir ordonné une dépense financière à un sous-agent. | L'émission de l'Agent Passport par le principal inclut la signature de l'empreinte constitutionnelle et des limites d'exécution. |
| **Information Disclosure** | Fuite latérale d'informations entre le tenant Famille et une organisation Enterprise hébergée sur la même machine. | Le pare-feu `CrossTenantFirewall` impose un cloisonnement étanche par défaut ; tout partage exige une table d'accords bilatéraux mutuels explicites. |
| **Denial of Service** | Injection de dépendances circulaires dans le graphe relationnel ReBAC pour bloquer l'évaluateur de permissions. | Détection de cycles via ensemble de chemin actif pendant la traversée DFS ; coupure et rejet en moins de 100 microsecondes. |
| **Elevation of Privilege** | Attaque du mandataire confus (*Confused Deputy*) où un sous-agent de profondeur 5 tente d'exécuter des actions avec l'autorité racine. | Plafonnement matériel strict de la profondeur de délégation $D \le 4$ et atténuation strictement monotone. |

---

## Contrats d'Interface Rust

### Définitions des Niveaux de Tenancy (`hakille-identity::hierarchy`)

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EntityTier {
    Team,
    Organization,
    Enterprise,
    Family,
    Group,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TenantId(pub Uuid);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantRecord {
    pub id: TenantId,
    pub tier: EntityTier,
    pub name: String,
    pub parent_id: Option<TenantId>,
    pub state: TenantState,
}
```

### Moteur ReBAC Zanzibar (`hakille-identity::rebac`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RelationTuple {
    pub entity_type: String,
    pub entity_id: String,
    pub relation: String,
    pub subject_type: String,
    pub subject_id: String,
}

pub struct RebacEngine {
    pub fn insert_tuple(&mut self, tuple: RelationTuple);
    pub fn remove_tuple(&mut self, tuple: &RelationTuple);
    pub fn check_relation(&self, entity_type: &str, entity_id: &str, relation: &str, subject_type: &str, subject_id: &str) -> bool;
    pub fn check_relation_transitive(&self, entity_type: &str, entity_id: &str, relation: &str, subject_type: &str, subject_id: &str) -> bool;
}
```

### Moteur d'Autorisation Hybride Contextuelle (`hakille-identity::access`)

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeviceTrustTier {
    Tier3UntrustedWeb,
    Tier2SoftwareHost,
    Tier1HardwareEnclave,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    pub subject: String,
    pub base_role: String,
    pub confidence_score: f64,
    pub device_tier: DeviceTrustTier,
    pub tenant_id: TenantId,
    pub scope: CapabilityScope,
    pub action: CapabilityAction,
    pub resource: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny(String),
}

pub struct AccessEngine {
    pub fn check_permission(&self, ctx: &AuthContext, tuple: &RelationTuple) -> Decision;
}
```

### Passeports d'Agents et Délégations Bicornées (`hakille-identity::agent`)

```rust
pub const MAX_DELEGATION_DEPTH: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentPassport {
    pub agent_did: Did,
    pub principal_did: Did,
    pub tenant_id: TenantId,
    pub max_lifetime_seconds: u64,
    pub max_execution_fuel: u64,
    pub budget_limit_cents: u64,
    pub biscuit_token_bytes: Vec<u8>,
    pub delegation_depth: usize,
    pub constitutional_hash: [u8; 32],
}

pub struct AgentFactory;

impl AgentFactory {
    pub fn spawn_agent() -> (ed25519_dalek::SigningKey, Did);
    pub fn issue_passport(
        agent_did: Did,
        principal_did: Did,
        tenant_id: TenantId,
        lifetime_seconds: u64,
        fuel: u64,
        budget_cents: u64,
        biscuit_token_bytes: Vec<u8>,
        delegation_depth: usize,
    ) -> Result<AgentPassport, HakilleError>;
}
```

### Protection des Familles, Mineurs et Invités (`hakille-auth::minor`, `hakille-auth::guest`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinorProfile {
    pub member: HouseholdMember,
    pub parent_user_ids: Vec<UserId>,
    pub age_category: AgeCategory,
}

pub struct GuestSession {
    pub guest_id: String,
    pub host_user_id: UserId,
    pub allowed_scopes: Vec<CapabilityScope>,
    pub purpose: String,
    pub expires_at: Timestamp,
    pub is_active: bool,
}

impl GuestSession {
    pub fn allows_scope(&self, scope: &CapabilityScope) -> bool; // Garanti false pour Wallet et FullSystem
}
```

---

## Conséquences et Garanties

### Positives
1. **Modélisation Naturelle des Relations Humaines et Logicielles** : Les 5 niveaux cardinaux permettent d'exprimer aussi bien la hiérarchie d'un groupe du CAC 40 que le partage d'une tablette familiale avec un enfant de 8 ans.
2. **Confinement Mathématique des Agents Autonomes** : Grâce au plafond $D_{\text{max}} = 4$ et à l'atténuation Datalog monotone de Biscuit, aucun essaim d'agents ne peut diverger ou usurper des pouvoirs financiers.
3. **Protection Inviolable des Mineurs au Niveau Système** : La Règle 7 NoWallet est garantie au niveau du système de types Rust, rendant impossible tout contournement par prompt injection ou manipulation UI.
4. **Cloisonnement Étanche des Données Personnelles** : L'airgap inter-tenant empêche les données intimes de la famille de fuiter dans les environnements de travail professionnels.

### Négatives et Compromis
1. **Latence de Traversée de Graphe** : Les requêtes ReBAC à profondeur variable imposent une mise en cache LRU en mémoire des tuples validés pour maintenir la latence d'évaluation sous la barre des 2 millisecondes.
2. **Complexité Opérationnelle des Passeports** : Le suivi des budgets et du fuel d'exécution des agents nécessite une synchronisation régulière avec le journal d'audit append-only de `hakille-audit`.
