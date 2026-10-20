# Plan de renommage — Annora vers Hakille

## Décision de marque

La marque publique, le produit et l'intelligence s'appellent **Hakille**.

> **Hakille — intelligence souveraine.**

`Hakille` est le terme de marque unique. `Hakille OS` est une description
technique possible, mais pas un second produit. Le mot s'appuie sur le nom de
l'entreprise et sur sa signification en peul communiquée par le fondateur :
l'intelligence.

## Architecture cible

| Surface | Cible | Règle |
| --- | --- | --- |
| Entreprise | `Hakille` | Nom légal et marque mère. |
| Produit | `Hakille` | Nom affiché à l'utilisateur. |
| Description | `Hakille OS` | Réservée aux contextes techniques. |
| Compte GitHub | `hakille` | Compte canonique du dépôt. |
| Dépôt principal | `hakille` | Monorepo Rust public. |
| Domaine principal | `hakille.ai` | Site produit et identité publique. |
| Documentation | `docs.hakille.ai` | Documentation publiée. |
| Application | `app.hakille.ai` | Interface web, si elle est exposée. |
| API | `api.hakille.ai` | API publique, si elle est exposée. |
| E-mails | `hello@hakille.ai`, `security@hakille.ai` | Contact et sécurité. |

Ne pas présenter `Annora` comme une seconde marque. Pendant la migration,
Annora est seulement un ancien identifiant technique.

## Principes de migration

1. La marque publique est migrée avant les identifiants Rust.
2. Les noms de crates sont migrés dans une seule série cohérente ; ne pas
   mélanger `annora-*` et `hakille-*` durablement.
3. Les données utilisateur, chemins de stockage et protocoles IPC doivent
   disposer d'une migration compatible et réversible.
4. Toute modification de protocole, de signature ou de stockage doit être
   versionnée et testée. Un renommage ne doit jamais rendre une mémoire ou un
   journal existant illisible.
5. Le dépôt est actuellement modifié sur de nombreux fichiers. Stabiliser ou
   isoler ces changements avant de lancer le renommage mécanique.

## Ordre d'exécution recommandé

### Phase 0 — Réserver et protéger l'identité

- Réserver `hakille.ai` en priorité, puis `hakille.com`, `hakille.dev` et
  `hakille.io` si le budget le permet.
- Configurer une boîte e-mail de domaine et conserver les preuves de
  réservation.
- Vérifier les marques dans les classes pertinentes, au minimum logiciel et
  services technologiques, avant toute annonce publique.
- Conserver l'identifiant GitHub `hakille` et sécuriser le compte
  (MFA, clés de récupération, e-mail de secours).

**Critère de sortie :** le domaine principal est contrôlé et la décision de
marque est documentée.

### Phase 1 — Créer le dépôt canonique

- Créer `github.com/hakille/hakille`.
- Ajouter le remote `origin` au checkout de travail après validation de l'URL.
- Mettre à jour les liens GitHub, badges et champs `repository` présents dans
  les manifests Cargo.
- Conserver l'ancien dépôt, s'il est publié ultérieurement, en lecture seule
  avec un README de redirection pendant au moins une version majeure.

**Critère de sortie :** un clone neuf du dépôt `hakille` construit le même
commit que le checkout de référence.

### Phase 2 — Marque et documentation visibles

Modifier en premier les éléments sans effet sur le format des données :

- titre et texte du `README.md` ;
- guides dans `docs/` et ADR ;
- titres de la CI, badges, métadonnées et captures d'écran ;
- libellés de l'interface, onboarding et messages affichés ;
- documentation de sécurité, politique de confidentialité et mentions légales
  lorsqu'elles existeront.

Utiliser la formule `Hakille — intelligence souveraine` de façon cohérente.
Ne pas promettre de capacités qui ne sont pas démontrées par les tests ou les
binaires réellement disponibles.

**Critère de sortie :** une recherche de `Annora` dans le contenu public ne
retourne que les notes de migration ou les compatibilités historiques.

### Phase 3 — Workspace Rust

Renommer les dossiers, le champ `package.name`, les dépendances et les imports
en une même modification atomique.

| Identifiant actuel | Identifiant cible |
| --- | --- |
| `annora-core` | `hakille-core` |
| `annora-substrate` | `hakille-substrate` |
| `annora-substrate-windows` | `hakille-substrate-windows` |
| `annora-substrate-linux` | `hakille-substrate-linux` |
| `annora-substrate-macos` | `hakille-substrate-macos` |
| `annora-substrate-redox` | `hakille-substrate-redox` |
| `annora-substrate-mobile` | `hakille-substrate-mobile` |
| `annora-substrate-web` | `hakille-substrate-web` |
| `annora-memory` | `hakille-memory` |
| `annora-kernel` | `hakille-kernel` |
| `annora-perception` | `hakille-perception` |
| `annora-inference` | `hakille-inference` |
| `annora-voice` | `hakille-voice` |
| — (nouveau) | `hakille-audio` |
| `annora-agents` | `hakille-agents` |
| `annora-audit` | `hakille-audit` |
| `annora-sync` | `hakille-sync` |
| `annora-auth` | `hakille-auth` |
| `annora-constitution` | `hakille-constitution` |
| `annora-ui` | `hakille-ui` |
| `annora-ui-shell` | `hakille-ui-shell` |
| `annora-ui-model` | `hakille-ui-model` |
| `annora-ui-platform` | `hakille-ui-platform` |
| `annora-ui-protocol` | `hakille-ui-protocol` |
| `annora-ui-desktop` | `hakille-ui-desktop` |
| `annora-design` | `hakille-design` |
| — (nouveau) | `hakille-ipc` |
| `annora-keystore` | `hakille-keystore` |
| `annora-metrics` | `hakille-metrics` |
| `annora-spikes` | `hakille-spikes` |
| `annora-daemon` | `hakille-daemon` |

Le nom du binaire utilisateur doit devenir `hakille` ou `hakille-daemon`.
Préférer `hakille` pour la commande publique si aucun autre binaire ne peut
porter ce nom.

Après le changement, régénérer `Cargo.lock` avec la procédure habituelle du
projet et vérifier qu'aucun chemin local ne pointe vers un ancien dossier.

**Critère de sortie :** `cargo test --workspace --locked` et les contrôles de
conformité passent avec les noms `hakille-*` uniquement.

### Phase 4 — Compatibilité des données et des intégrations

Ces identifiants ne doivent pas être remplacés aveuglément :

- dossiers de données, par exemple `annora-memory.json` ;
- chemins système (`AppData`, macOS `Library`, Linux XDG) ;
- noms IPC, sockets, services, tâches planifiées et clés de registre ;
- sujets de messages, schémas JSON, signatures, clés de chiffrement et audit ;
- noms de manifestes d'agents WASM et identifiants de capabilities ;
- variables d'environnement, options CLI et fichiers de configuration.

Politique requise : au premier démarrage de Hakille, détecter les données
Annora, proposer une copie ou migration explicite, journaliser le résultat,
et ne supprimer aucune donnée source automatiquement. Accepter les anciens
formats pendant une période documentée ou fournir un outil d'export/import.

**Critère de sortie :** une installation Hakille peut ouvrir, migrer et vérifier
un jeu de données Annora de référence sans perdre le journal, la mémoire ou les
droits.

### Phase 5 — Publication

- Publier une note `RENAMING.md` ou une section de changelog : Annora devient
  Hakille, sans changement de propriétaire des données.
- Mettre en place les redirections `annora` vers `hakille.ai` si des domaines
  historiques sont détenus.
- Mettre à jour les liens externes, pages de téléchargement, images de release
  et signatures de binaires.
- Ne supprimer les alias de compatibilité qu'après une date annoncée.

## Recherches mécaniques avant chaque phase

Exécuter les recherches suivantes et traiter chaque résultat selon son type :

```powershell
rg -n -i "annora" -g "!target/**"
rg -n -i "annora-memory|annora-daemon|annora-" -g "!target/**"
rg -n -i "com\\.annora|/Annora|AppData.*Annora" -g "!target/**"
```

Classer les résultats en :

- **marque visible** : renommer immédiatement ;
- **identifiant Rust** : renommer pendant la phase Workspace ;
- **format ou protocole** : migrer avec compatibilité et tests ;
- **preuve historique** : conserver seulement dans le changelog ou cette note.

## Validation finale

Avant publication :

- [ ] domaine et identité GitHub contrôlés ;
- [ ] README, documentation et UI affichent Hakille ;
- [ ] workspace sans crates `hakille-*` ;
- [ ] CI et conformance passent ;
- [ ] ancienne mémoire et ancien journal testés ;
- [ ] binaires, chemins système et IPC migrés ;
- [ ] aucun secret ni identifiant de production n'a été modifié accidentellement ;
- [ ] note de migration publiée.

## Décision à prendre avant d'éditer le code

Le prochain changement doit être choisi explicitement :

1. **Migration publique seulement** : documentation, GitHub et domaine deviennent
   Hakille ; les noms Rust restent temporairement `hakille-*`.
2. **Renommage complet** : marque, code Rust, binaires, chemins et migrations de
   données changent dans une série de commits dédiée.

La première option est plus sûre pour annoncer la marque rapidement. La seconde
est la cible finale, mais elle doit être faite après stabilisation du checkout
actuellement modifié.
