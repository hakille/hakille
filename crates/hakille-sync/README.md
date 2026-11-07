# Synchronisation locale : contrats et limites

Ce crate fournit une fusion de données, le chiffrement de messages et un
transfert de contexte signé. Il n'établit aucun transport réseau et ne réalise
aucun appairage automatique. Aucun handoff n'exécute une action métier.

## Handoff signé, protocole v2

`HandoffCoordinator::verify_handoff` exige un couple identité/clé explicitement
approuvé avec `approve_peer(device_id, public_key)`. Cette décision doit provenir
d'un appairage hors bande ou d'un magasin local de confiance. **Ne jamais appeler
`approve_peer` sur la seule base de `coordinator_pubkey` reçu dans un message.**
La clé du message doit correspondre à celle déjà approuvée. Une rotation impose
une révocation explicite ; la révocation efface aussi le contexte actif reçu
de ce pair. Au plus 128 pairs sont approuvés par coordinateur.

Les signatures v2 incluent le destinataire, le contenu, les champs optionnels
avec leur présence (`None` diffère de `Some("")`), le token et les dates. Les
messages v1 restent désérialisables mais sont refusés. `SYNC_PROTOCOL_VERSION`
vaut 2 ; une version absente reste 1, sans promotion implicite. Le transport
hôte doit vérifier cette compatibilité avant tout échange.

Les messages expirés, de durée supérieure à cinq minutes, émis plus de trente
secondes dans le futur, trop volumineux, altérés ou destinés à un autre appareil
sont refusés. Le cache anti-rejeu retient au plus 4 096 couples source/token
jusqu'à leur expiration. Une saturation refuse la réception ; elle n'évince
pas un token encore valide. `verify_handoff` vérifie sans consommer ;
`receive_handoff` consomme le token et conserve le contexte accepté.
Le TTL est contrôlé à la réception : `current_state()` conserve ensuite ce
contexte même après son expiration. Sa présence ne prouve pas une session active.

`new` crée une clé éphémère : un redémarrage requiert un nouvel appairage.
`with_keypair` permet à l'hôte de fournir une clé persistée dans son stockage
sécurisé. Pour préserver l'anti-rejeu après redémarrage, écrire atomiquement et
durablement `replay_snapshot_json()` **avant d'utiliser le contexte reçu**, puis
appeler `restore_replay_snapshot_json()` avant toute nouvelle réception. La
restauration est validée, additive et ne crée aucune approbation de pair. Le
crate ne protège pas le fichier contre la falsification ou le rollback : ces
garanties appartiennent au stockage de l'hôte. Aucun chemin de production ne
persiste automatiquement ce cache à ce jour.

## Fusion de données et suppressions

`CrdtSync` utilise une fusion remove-wins/LWW : une tombstone prime sur une
présence ; à ID égal, le maximum de `(last_activated, représentation JSON du
Triplet complet à champs fixes)` est retenu. Un timestamp égal ne dépend plus
de l'ordre des pairs. Les valeurs numériques non finies sont refusées avant
comparaison. Les tests vérifient les valeurs finales et les six permutations
de trois états, avec regroupement et idempotence.

Cette fusion choisit **une** version et ne conserve pas les versions perdantes.
L'horloge vectorielle globale n'établit pas la causalité de chaque élément.
Le schéma canonique et les règles de fusion doivent rester compatibles entre
pairs ; ce mécanisme ne constitue pas une garantie de fusion métier sans perte.

`VectorClock::tick`, `merge`, `insert_triplet` et `remove_triplet` retournent des
erreurs explicites. Un dépassement de compteur, une horloge invalide, une
valeur non finie ou un état incohérent est refusé sans mutation partielle.
Les horloges sont limitées à 1 024 identités. Un ID supprimé ne peut pas être
réinséré sans protocole de génération causale ; créer un nouvel ID.

`gc` et `gc_older_than` retournent désormais zéro et conservent les tombstones.
L'âge seul ne prouve pas qu'un pair hors ligne a reçu une suppression. Une
purge sûre nécessiterait des accusés de convergence ou un protocole de
compaction avec exclusion des anciens états. Aucun tel protocole n'est livré ;
la croissance des tombstones reste donc une limite explicite. Les tombstones
historiques sans date conservent cette absence plutôt que recevoir une date
locale inventée à chaque fusion.

## Réplication

`build_delta` et les variantes chiffrées envoient toujours l'état actif complet
et les tombstones. Les anciens arguments `from_clock` et `since` sont conservés
mais ne filtrent plus les données : une horloge globale égale ne prouve pas
l'égalité des contenus, et `created_at` manque les modifications ultérieures.
Ce repli privilégie la convergence ; aucun delta réseau efficace n'est annoncé.
Les messages chiffrés sont limités à 32 Mio de JSON avant chiffrement. Les IDs
dupliqués dans un même message sont refusés avant modification du récepteur.

`apply_encrypted_delta` vérifie l'authenticité AEAD sous la clé symétrique
fournie, donc la possession de cette clé ; il ne lie pas le ciphertext à
`from_device_id`, à la version du protocole ou au destinataire. Il n'applique
pas non plus de politique d'import aux champs `TripletSource` et `TrustLevel`.
Le transport hôte doit authentifier et autoriser le pair, vérifier ces liens
et définir le niveau de confiance permis aux données importées avant la fusion.

Le pont `knowledge_graph_to_crdt` retourne maintenant `Result<CrdtSync>`.
`apply_crdt_to_graph` rejoue les seuls triplets actifs : il ne propage pas les
suppressions vers le graphe et conserve les conflits métier existants.

Validation ciblée :

```bash
CARGO_INCREMENTAL=0 cargo test -p hakille-sync --locked
CARGO_INCREMENTAL=0 cargo clippy -p hakille-sync --all-targets --no-deps --locked
```
