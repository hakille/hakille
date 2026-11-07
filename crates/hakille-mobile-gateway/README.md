# Passerelle mobile locale

Ce binaire optionnel expose HTTP/1 sur **127.0.0.1 uniquement** et relaie les
requêtes vers le vrai démon IPC local. Il n'exécute pas de noyau ni de modèle
supplémentaire. `/health` indique seulement que la passerelle répond ; il ne
prétend pas que le démon ou un modèle est disponible.

Le démon actuel sert une seule connexion IPC persistante à la fois.
**Quitter/déconnecter le HUD desktop avant de connecter le client mobile.**
La passerelle conserve cette connexion entre les appels HTTP ; les cartes lui
sont liées. Ce lot n'ajoute pas de concurrence côté démon ni de sessions
indépendantes pour plusieurs utilisateurs mobiles.

## Démarrer explicitement

```bash
cargo build -p hakille-mobile-gateway --locked
# Démarrer séparément hakille-daemon --serve, puis :
target/debug/hakille-mobile-gateway --listen 127.0.0.1:17840
```

Options :

- `--listen` : défaut `127.0.0.1:17840` ; toute autre adresse IP est refusée.
- `--ipc-endpoint` : défaut IPC de la plateforme, avec l'override
  `HAKILLE_IPC_ENDPOINT` existant.
- `--ipc-token-file` : défaut IPC de la plateforme, avec l'override
  `HAKILLE_IPC_TOKEN_FILE` existant.
- `--token-file` : défaut `mobile-gateway.token` à côté du fichier de jeton IPC.
  Il doit être distinct du jeton du démon.
- `--request-timeout-secs` : défaut **680 s**, maximum 900 s. Ce défaut laisse
  une marge au timeout Qwen de 600 s et au budget du noyau. Les applications
  natives utilisent 700 s ; augmenter leur délai si le serveur est configuré
  au-delà du défaut, en gardant une marge pour connexion/corps/réponse.
- `--session-idle-secs` : défaut 900 s, maximum 3600 s. L'expiration ferme
  l'IPC et invalide les cartes. Une requête en cours n'est pas une session idle.

Le fichier mobile contient un **autre secret** que celui du démon : 32 octets
aléatoires de l'OS, encodés en 64 caractères hexadécimaux. Il est renouvelé à
chaque démarrage, après un bind HTTP réussi ; le téléphone doit être réapparié
après renouvellement. Seul son chemin est affiché, jamais son contenu.
La publication utilise le helper IPC atomique, fichier créé en mode 0600 sur
Unix. Sous Windows, les ACL du répertoire parent s'appliquent.

Les répertoires de jetons doivent rester sous le contrôle de l'utilisateur.
La lecture du jeton IPC est bornée à 256 octets et refuse les liens/fichiers
non ordinaires. Sur Unix, `O_NOFOLLOW | O_NONBLOCK` et les métadonnées du handle
ouvert empêchent de suivre un lien ou de bloquer sur un FIFO substitué ; les
permissions de groupe/autres sont refusées. Sous Windows, le fichier est ouvert
avec `FILE_FLAG_OPEN_REPARSE_POINT`, puis ses attributs de handle sont vérifiés :
un point de réanalyse substitué n'est pas suivi. Un fichier existant est aussi
canonicalisé entièrement lors du contrôle des destinations, pour que des noms
comme `IPC.TOKEN` et `ipc.token` ne puissent pas écraser le même jeton de démon
sur un volume insensible à la casse. Ce mécanisme n'est pas un coffre
contre un administrateur ou un processus possédant déjà les droits du compte.

## Contrat HTTP natif v1

Pas de redirection, de CORS ni de navigateur : toute requête portant `Origin`
est refusée. Les paramètres de query sont refusés, notamment pour éviter les
jetons dans les URL. Le secret n'est accepté que dans :

```text
Authorization: Bearer <jeton mobile>
Content-Type: application/json
```

`GET /health` est la seule route non authentifiée et renvoie
`{"status":"ok"}`. Elle ne livre ni statut du démon ni identifiant de session.

`POST /v1/request` reçoit directement un `hakille_ipc::Frame::request` :

```json
{"version":1,"id":1,"body":{"Request":{"message":"StatusRequest"}}}
```

L'identifiant doit être compris entre 1 et `u64::MAX - 1`. La version IPC est
vérifiée. L'allowlist expose `HudSubmit`, `StatusRequest`, `JournalRequest`,
`MemoryRequest`, `MemoryRemember`, `MemoryForget`, `AgentListRequest`,
`AgentRunRequest`, `Approve`, `Edit` et `Reject`. Les autres messages sont
refusés avant envoi. Les textes libres/entrée agent sont limités à 64 Kio,
les listes à 200 éléments demandés ; le corps HTTP complet reste limité à 1 Mio.

Une réponse HTTP 200 a la forme :

```text
{ "session_id": "UUID", "frames": [ Frame, ... ] }
```

Le client doit valider l'enveloppe entière, appliquer **toutes les trames dans
l'ordre**, y compris celles d'identifiant 0 après la réponse finale, et mémoriser
la session uniquement pour ce hub. Un `FrameBody::Error` du démon reste une
erreur métier/protocole réelle dans cette liste, pas un succès métier.

Pour `Approve`, `Edit` ou `Reject`, ajouter :

```text
X-Hakille-Session: <session_id de la carte>
```

Sans session correspondante, HTTP 409 `stale_session`, aucun envoi IPC. Un
changement de session doit vider les cartes et autres données dépendantes de
l'ancienne session. Le header est aussi vérifié lorsqu'il accompagne une
autre requête. Après expiration ou erreur, effectuer une nouvelle consultation
sans ancien header pour établir une nouvelle session ; ne pas rejouer une
mutation dont l'issue est inconnue.

### Réponse finale et cartes de suivi

Le démon peut écrire `HudReply(id)` **avant** `ShowCard(0)`. Une lecture arrêtée
au premier identifiant correspondant perdrait cette carte. La passerelle écrit
donc ensuite `StatusRequest(u64::MAX)` sur le même flux séquentiel. Sa réponse
sert de borne déterministe, puis est retirée du résultat public. Il n'y a pas
de délai arbitraire de collecte des suivis.

Les `TokenDelta(0)` adjacents sont concaténés exactement, afin de respecter la
borne de 256 trames sans perdre de texte. `HudReply` reste la réponse textuelle
de référence. La réponse HTTP est **groupée**, au plus 1 Mio : les deltas ne
sont pas diffusés en direct par SSE/WebSocket. Une interface doit afficher son
propre indicateur d'attente durant l'appel.

### Erreurs et absence de rejeu

```text
{
  "session_id": "ancienne-session-ou-null",
  "frames": [],
  "error": {
    "code": "ipc_transport",
    "message": "message lisible",
    "issue_unknown": true
  }
}
```

- 400 : version, trame, identifiant ou requête refusée, sans envoi métier.
- 401 : authentification refusée avant lecture/parsing du corps.
- 403 : header `Origin` refusé.
- 408/413/415 : corps expiré, trop volumineux/illisible, mauvais type.
- 409 : session absente/périmée pour la requête concernée.
- 429 : une autre requête IPC est déjà en cours ; aucune mise en file cachée.
- 503 : connexion/handshake IPC indisponible, sans envoi métier.
- 502/504 après début d'envoi : **issue inconnue**, aucun rejeu automatique,
  connexion IPC fermée. Un dépassement de réponse ou une incohérence protocole
  produit aussi cette issue conservatrice.

Une future d'échange annulée abandonne son flux IPC : celui-ci est retiré de
l'état partagé avant l'envoi et réinséré seulement après un échange complet.
La fermeture du transport **n'interrompt pas nécessairement l'inférence ou un
effet déjà engagé dans le démon**. Le client ne doit pas en déduire un échec
métier ni réessayer la mutation. Une réponse HTTP perdue côté téléphone peut
également laisser son issue inconnue, même si le démon avait terminé.

## Accès depuis un téléphone

`127.0.0.1` sur le téléphone désigne le téléphone. Pour un appareil Android
relié en développement, un transfert explicite comme
`adb reverse tcp:17840 tcp:17840` peut relier ce port au poste. Cette commande
n'est pas exécutée automatiquement par Hakille.

Pour un accès distant, installer/configurer explicitement un tunnel ou un
reverse proxy **HTTPS** vers `127.0.0.1:17840`, avec certificat vérifié par les
clients natifs. Le proxy doit conserver `Authorization` et
`X-Hakille-Session`, accepter les corps de 1 Mio et les délais longs, ne pas
journaliser les secrets et ne pas rejouer les requêtes mutantes. Rien n'est
publié ni déployé par cette crate. Aucun mode `0.0.0.0`, HTTP distant ou
contournement de vérification TLS n'est fourni.

## Vérification

```bash
cargo test -p hakille-mobile-gateway --locked
cargo clippy -p hakille-mobile-gateway --all-targets --no-deps --locked
```

Les tests utilisent de vraies connexions HTTP locales et un faux démon IPC
explicitement identifié : authentification/Origin, versions/allowlist,
limites avec Content-Length et chunked, timeout de corps, ShowCard tardive,
300 deltas regroupés sans perte, session persistante/approbation, requête busy,
timeout après envoi, déconnexion sans replay, expiration, réponse incohérente,
annulation de future et fichiers de jetons. Ils ne prouvent pas une exécution
métier ni un déploiement natif sur téléphone ; les smokes contre le vrai démon
doivent être rapportés séparément.

### Contrat façade mobile → HTTP → IPC

Les scénarios versionnés dans
[`tests/scenarios/mobile_bridge.rs`](tests/scenarios/mobile_bridge.rs)
utilisent la vraie `MobileBridge`, de vrais sockets HTTP et IPC, et un noyau
contrôlé explicitement fictif. Ils ne remplacent pas les tests d'inférence ou
de métier contre un vrai démon. Ils couvrent :

- Statut, mémorisation/lecture de fixture, journal, agents, demande d'exécution
  de fixture, dialogue avec deltas et cartes après la réponse terminale.
- Refus d'une approbation prématurée, maintien de 600 ms, approbation et rejet
  avec session conservée et accusés typés.
- Fermeture de session et rotation du jeton IPC : décision périmée refusée
  avant émission, cartes effacées par la façade, puis nouveau statut sans ancien
  header et nouvelle session. Aucun cache de l'ancien jeton n'est réutilisé.
- Concurrence : `busy` est un refus sans émission, tandis qu'un timeout après
  émission devient une issue inconnue. La façade bloque une nouvelle mutation
  et aucun rejeu n'arrive au noyau contrôlé.
- Déconnexion TCP réelle avant réponse : fermeture IPC avant son timeout,
  issue inconnue conservée dans la façade, puis consultation distincte.
- Socket Unix périmé : erreur `ipc_unavailable` et conservation de l'objet
  filesystem, sans suppression automatique susceptible de toucher un démon.

La suite ciblée comporte actuellement quinze tests de transport/intégration
et un test du binaire sur les alias de fichiers de jetons. Les scénarios Unix
ne prétendent pas valider les named pipes sous Windows : ces exécutions
nécessitent leur hôte natif. Les tests compilables spécifiques à Windows
conservent un refus explicite/`SKIP` si le répertoire est sensible à la casse.

### Reprendre après redémarrage ou socket périmé

1. Après une coupure pendant une mutation, conserver l'issue inconnue et ne
   pas renvoyer la mutation. Une lecture de statut/journal peut aider à
   rapprocher l'état réel ; sa réussite ne prouve pas rétrospectivement l'effet.
2. Une erreur `stale_session` doit effacer le header de session et les cartes
   locales. Le prochain `refresh_status` part sans ce header. L'ancienne carte
   ne peut pas être approuvée dans la nouvelle session.
3. Si le démon a redémarré, sa nouvelle clé IPC est relue au prochain handshake.
   Un redémarrage de la passerelle renouvelle aussi son propre bearer : mettre
   à jour le secret mobile par le canal d'appairage prévu. Un 401 n'autorise pas
   à utiliser le jeton IPC comme jeton mobile.
4. Sous Unix, un socket peut rester après un crash. Vérifier que le démon
   correspondant est réellement arrêté avant tout nettoyage manuel. La
   passerelle ne supprime ni socket ni jeton de démon. Sous Windows, l'absence
   du named pipe est également une indisponibilité, pas un ordre de démarrage.

### Probe réutilisable contre un hub déjà démarré

L'exemple [`mobile_probe`](examples/mobile_probe.rs) traverse la façade mobile
et HTTP contre un hub explicitement choisi. Il ne démarre aucun démon, ne fait
que quatre consultations (statut, mémoire, journal, agents) et refuse les
mutations même si une future modification de la façade en proposait une.

```bash
cargo run -p hakille-mobile-gateway --example mobile_probe --locked -- \
  --hub http://127.0.0.1:17840 \
  --token-file /chemin/prive/mobile-gateway.token
```

Pour un hub distant, HTTPS avec vérification de certificat reste obligatoire.
Le probe refuse les redirections, les credentials/query/fragment dans l'URL et
les retries HTTP automatiques. La réponse est bornée à 1 Mio. Le rapport JSON
contient les états des consultations, leurs durées et les nombres de lignes,
jamais le bearer ni le contenu de la mémoire ou du journal. Ce rapport est
produit par un appel réel ; aucun résultat de démon n'est inclus en dur dans
l'exemple. Une consultation refusée donne un code de sortie non nul.
