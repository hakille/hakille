# Cœur mobile Hakille

Cette crate fournit la machine d'état commune aux hôtes Android Compose et
iOS SwiftUI. Elle consomme les messages complets de `hakille-ui-protocol` et
produit des `hakille_ipc::Frame`. Elle n'embarque ni renderer desktop, ni
CPAL, ni ONNX, ni modèles ; elle n'ouvre aucun réseau ou microphone.

## API native stable

`MobileBridge` possède les méthodes Rust suivantes, exposées par UniFFI
avec les noms natifs générés :

```text
new() -> MobileBridge
snapshot_json() -> String
dispatch_json(action_json: String) -> String
receive_json(request_id: u64, response_json: String) -> String
fail_request(request_id: u64, message: String) -> String
draft_json() -> String
restore_draft_json(draft_json: String) -> String
```

`snapshot_json` retourne directement le snapshot ; `draft_json` retourne
directement le format de brouillon. Les autres opérations retournent :

```json
{"ok":true,"error":null,"request":null,"snapshot":{}}
```

Le snapshot complet est défini dans [types.rs](src/types.rs). `ok` signifie
que l'opération du bridge a été traitée : **il ne signifie jamais qu'un
effet métier a été exécuté**. Consulter `outcomes[].state`, le résultat typé,
l'identifiant d'audit et `CardDecisionAck.effect` pour connaître la réponse
réelle du hub.

Exemples d'actions :

```json
{"type":"set_hub","hub_id":"mon-hub"}
{"type":"set_draft","text":"Bonjour Hakille"}
{"type":"submit"}
{"type":"refresh_status"}
{"type":"load_memory","query":null,"limit":50}
{"type":"remember","text":"Fait à mémoriser"}
{"type":"load_journal","limit":50}
{"type":"load_agents"}
{"type":"run_agent","name":"echo-agent","input_json":"{}"}
{"type":"begin_approval","card_id":7}
{"type":"approve","card_id":7}
{"type":"cancel_approval"}
{"type":"reject","card_id":7}
{"type":"set_foreground","active":false}
{"type":"acknowledge_unknown","request_id":7}
{"type":"fail_before_send","request_id":8,"message":"Sauvegarde locale impossible"}
```

Une action inconnue, un champ inattendu ou une valeur hors limites produit
une erreur. `set_microphone` avec `enabled:true` est explicitement refusé :
aucun binding de capture n'est raccordé à cette crate. `enabled:false`
conserve l'état éteint, sans prétendre arrêter un appareil qu'elle ne possède
pas. L'hôte doit gérer ses ressources natives réelles.

## Transport et réponses

L'hôte transmet le champ `request`, lorsqu'il est présent, au gateway
`POST /v1/request`. Exemple de frame :

```json
{"version":1,"id":1,"body":{"Request":{"message":"StatusRequest"}}}
```

Il restitue le corps complet de la réponse à `receive_json`, avec l'ID
conservé lors de l'envoi :

```json
{
  "session_id":"session-opaque",
  "frames":[
    {"version":1,"id":1,"body":{"Reply":{"message":{"StatusReply":{"status":{"model":"Qwen","cost":"0 EUR","state":"Idle","memory_count":0}}}}}}
  ]
}
```

Une erreur gateway utilise
`{"session_id":null,"frames":[],"error":{"code":"…","message":"…","issue_unknown":true}}`.
Un échec réseau sans réponse passe par `fail_request`. Ni les messages
d'erreur ni `hub_id` ne doivent contenir de credentials. Les identifiants
de session reçus sont restreints aux caractères sûrs d'un en-tête HTTP.

Le transport appartient à l'hôte : authentification Bearer, protection de
ses credentials et configuration TLS ne sont pas implémentées dans ce
bridge. Pour les décisions, `snapshot.gateway_session` fournit la valeur
de `X-Hakille-Session` exigée par le gateway. `hub_id` identifie le contexte,
sans accorder de permission réseau. Il refuse userinfo, query et fragment
pour éviter d'y conserver accidentellement des secrets d'URL.

Le gateway regroupe les frames : ce client applique les deltas et les
suivis dans leur ordre à réception du batch ; il ne revendique pas un flux
HTTP de tokens en direct. `HudReply` remplace les deltas provisoires ; un
delta après la réponse finale est refusé. Une `ShowCard` après `HudReply`
reste prise en compte. Le batch est prévalidé et appliqué atomiquement.

## Invariants

- Une seule requête en vol. IDs monotones `1..u64::MAX-1` ; `u64::MAX`
  reste réservé à la barrière du gateway, sans overflow ni retour à zéro.
- JSON entrant limité à 1 Mio et batch à 256 frames. Validation des versions,
  IDs et types de réponse ; une réponse ancienne ne clôture pas la demande
  courante. Préserver les `u64` natifs sans conversion en `Double`.
- Snapshot limité à 512 Kio. Brouillon : 8192 octets UTF-8 ; listes : 200
  lignes ; cartes : 32 ; chat : 64 entrées ; résultats : 32 ; notifications :
  16. Les anciennes entrées historiques sont retirées si le budget d'octets
  l'exige, avec `history_trimmed=true`. Une liste entrante trop grande est
  refusée au lieu de se présenter comme complète après troncature.
- Hub changé ou session remplacée : cartes invalidées. Une mutation dont
  le résultat est ambigu reste `unknown`, sans rejeu automatique. Seule une
  nouvelle réponse de statut peut rétablir la connexion après cet état ;
  elle ne transforme pas l'ancien résultat inconnu en succès. Les nouvelles
  mutations restent bloquées tant que `recovery_required` vaut `true` ; les
  consultations restent disponibles. Une prise de connaissance explicite
  de chaque avertissement est nécessaire pour lever ce blocage.
- Un toast libre n'est pas un accusé d'effet pour une mutation. Les résultats
  mémoire/agents/cartes sont conservés dans leurs DTO complets, avec audit
  et effet lorsqu'ils sont fournis.
- Approbation : carte actuelle, session confirmée et délai de 600 ms mesuré
  avec `Instant` en Rust. L'hôte commence au toucher, approuve au relâchement
  et annule au retrait du doigt, interruption ou changement de contexte.
  Une interaction d'accessibilité en deux activations doit être annoncée
  comme telle : le bridge mesure un délai, pas la présence physique d'un
  doigt. Cette vérification locale n'est pas une attestation cryptographique
  adressée au serveur. Le refus ne nécessite pas ce délai.
- Passage en arrière-plan : confirmation annulée, demande en vol abandonnée
  avec résultat incertain si nécessaire, microphone éteint. La reprise ne
  redémarre automatiquement ni microphone, ni demande réseau.

## Checkpoint durable et reprise après interruption

`draft_json()` retourne un checkpoint **v2** minimal. Les signatures FFI
restent inchangées ; les fichiers v1 `{schema_version:1,draft:"…"}` restent
lisibles. La restauration v1 ne peut pas effacer les incertitudes déjà connues
d'une instance et ne peut pas reconstruire des opérations que l'ancien
format n'avait jamais sauvegardées.

```json
{
  "schema_version": 2,
  "draft": "Brouillon conservé",
  "next_request_id": 12,
  "uncertain_operations": [
    {"request_id": 11, "kind": "remember", "phase": "in_flight"}
  ],
  "acknowledged_operations": []
}
```

Les marqueurs contiennent uniquement l'identifiant monotone, le type de
mutation (`submit`, `remember`, `run_agent`, `approve`, `reject`) et la phase
(`in_flight` ou `unknown`). Ils ne contiennent ni payload de commande, ni
chat, ni hub, ni session, ni token, ni carte. Le brouillon reste du texte
personnel : l'hôte doit le protéger dans son stockage privé. La crate
n'effectue elle-même aucune écriture de fichier.

L'hôte doit respecter cet ordre :

1. Traiter l'action avec `dispatch_json` ; un marqueur est créé avant que
   la trame de mutation soit retournée.
2. Publier **durablement et atomiquement** `draft_json()` avant tout envoi
   HTTP. Sérialiser les écritures et empêcher une sauvegarde ancienne de
   remplacer une version récente. Une erreur de sauvegarde interdit l'envoi.
3. Si aucun envoi n'a commencé, utiliser l'action locale `fail_before_send`
   pour constater `failed` / « Non envoyée » et retirer le marqueur. Cette
   action repose sur l'attestation de l'hôte : elle est interdite après le
   début d'un envoi, même si le transport retourne ensuite une erreur.
4. Après réponse, transmettre l'enveloppe entière à `receive_json`, puis
   publier le nouveau checkpoint. Après coupure du transport, appeler
   `fail_request` et sauvegarder également. Si cette dernière sauvegarde
   échoue, l'ancien marqueur durable reste volontairement conservateur.
5. Au démarrage, restaurer le fichier avant de permettre les interactions.
   Une corruption doit être signalée sans écraser le fichier. Restaurer
   ne déclenche aucun réseau, microphone ou renvoi de demande.

Une réponse typée valide ou un refus connu avant envoi retire le marqueur.
Une interruption ambiguë le passe à `unknown`. Au chargement, tout
`in_flight` devient `unknown` : `pending` reste vide, les cartes et la
session sont invalidées, et un avertissement est affiché. Le marqueur n'est
pas une commande durable et ne contient pas les données pour la rejouer.

Le snapshot expose `recovery_required`, `uncertain_operations` et
`acknowledged_operations`. Après une nouvelle connexion, l'utilisateur peut
consulter le statut et le journal. **Un `StatusReply` n'est aucune preuve de
l'effet de l'ancienne opération.** Une mutation reste bloquée tant qu'un
avertissement n'a pas été lu explicitement. L'action locale
`acknowledge_unknown {request_id}` archive cette prise de connaissance,
sans réseau ni annulation ; l'outcome reste `state:"unknown"` avec
`acknowledged:true`, jamais `confirmed`. L'interface doit annoncer ce sens
avant de proposer l'action. Un changement de hub conserve les avertissements,
qui peuvent donc concerner un hub précédent non identifié dans le checkpoint.

Le format est limité à 64 Kio, 32 marqueurs actifs et 32 archives de prise de
connaissance. Le compteur et les IDs sont validés, y compris aux bornes u64 ;
`u64::MAX` reste un compteur épuisé, sans réutilisation d'IDs. Les archives
les plus anciennes sont évincées au-delà de 32 : elles ne constituent pas un
journal d'audit permanent. Restaurer plusieurs fois le même checkpoint est
idempotent ; un ancien marqueur encore présent dans les archives locales ne
réarme pas l'avertissement. Le bridge refuse les versions/champs inconnus,
IDs dupliqués, incohérences et états contradictoires sans modifier l'état
courant. Aucune restauration n'est acceptée pendant une requête en cours.

Un hôte qui recrée son `MobileBridge` lors d'une connexion/déconnexion doit
interrompre la requête précédente, exporter le checkpoint le plus récent,
puis le restaurer dans la nouvelle instance avant `set_hub`. Recréer un
bridge vide ferait perdre les avertissements et le compteur. Le texte soumis
reste dans le brouillon jusqu'à la réponse finale ; une modification faite
pendant l'attente n'est pas effacée par cette réponse.

## Construction et vérification

```bash
cargo test -p hakille-mobile --locked
cargo test -p hakille-mobile --features ffi --locked
cargo clippy -p hakille-mobile --all-targets --all-features --no-deps --locked
cargo build -p hakille-mobile --features ffi --locked
cargo build -p hakille-mobile --features cli --bin hakille-mobile-bindgen --locked
```

UniFFI est épinglé à `0.32.2`. `ffi` expose l'objet ; `cli` ajoute son
générateur. Les sorties Rust sont `rlib`, `cdylib` et `staticlib`. La
configuration locale [uniffi.toml](uniffi.toml) fixe le package Kotlin
`uniffi.hakille_mobile`, le module Swift `HakilleMobile` et son module C
`HakilleMobileFFI`. Le script de build natif du dépôt est responsable de la
génération et de la copie des bindings et bibliothèques de chaque cible.

Les tests Rust couvrent des gateways de fixture, le protocole, les bornes,
la corrélation, les interruptions et les confirmations. Ils ne prouvent pas
l'exécution d'une app native, le réseau sur appareil, les permissions,
VoiceOver/TalkBack ou les signatures de distribution.
