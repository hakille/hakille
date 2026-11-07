# Diagnostiquer le microphone

La capture utilise CPAL sur le système hôte. Elle fournit des blocs de **160
échantillons mono f32 à 16 kHz**, soit 10 ms, après conversion du format natif.
Le nom du périphérique et sa configuration annoncée ne prouvent ni l'autorisation
d'enregistrer, ni la réception d'audio, ni la reconnaissance de « Hakille ».

## Vérifier l'entrée choisie

```sh
cargo run --locked -p hakille-daemon -- voice devices
```

L'inventaire doit permettre de distinguer le périphérique par défaut, une
configuration disponible et une erreur du pilote. Il ne démarre pas la capture
et ne mesure pas le niveau du microphone. Un micro Bluetooth peut apparaître
sans que son profil d'entrée soit disponible.

`Capture::new()` utilise exclusivement l'entrée par défaut du système. Un échec
sur cette entrée ne déclenche pas un basculement silencieux vers une webcam,
une entrée virtuelle ou un autre micro. `Capture::with_device_name()` exige le
nom exact ; un nom absent ou ambigu est refusé. En cas de noms identiques,
choisir le périphérique par défaut dans les réglages du système.

Le format f32 est préféré sur le périphérique choisi, puis le nombre de canaux
le plus faible et les fréquences 16 / 48 / 44,1 kHz. Les entrées entières restent
convertibles. Une configuration annoncée peut encore être refusée au démarrage
(permission, périphérique occupé, débranchement ou configuration non ouvrable).

## Windows

1. Dans **Paramètres → Système → Son → Entrée**, sélectionner le micro et
   vérifier que son indicateur réagit à la parole.
2. Dans **Confidentialité et sécurité → Microphone**, autoriser l'accès au
   microphone et l'accès des applications de bureau. Vérifier le bouton muet
   matériel du casque.
3. Fermer une application qui monopolise le périphérique en mode exclusif.
   Pour un casque Bluetooth, vérifier que le profil avec microphone est actif.
4. Relancer la capture après changement du périphérique ou de ses permissions.

Un nom correctement listé n'atteste pas que Windows autorise sa capture.

## macOS

1. Dans **Réglages Système → Son → Entrée**, choisir le micro et vérifier le
   niveau d'entrée et son volume.
2. Dans **Confidentialité et sécurité → Microphone**, autoriser l'application
   qui lance Hakille : Terminal ou l'IDE pour une commande de développement,
   puis l'application empaquetée pour son exécution native.
3. Relancer l'application après modification de la permission si nécessaire.
   La permission du Terminal ne vaut pas preuve pour un autre lanceur.

Ne pas réinitialiser globalement les permissions de confidentialité pour
contourner un refus : vérifier d'abord l'application concernée.

## Linux

1. Dans les réglages audio de la session, choisir l'entrée, désactiver son muet
   et vérifier le niveau réel. Avec PipeWire, `wpctl status` et
   `wpctl get-volume @DEFAULT_AUDIO_SOURCE@` aident à identifier la source ;
   avec PulseAudio, utiliser `pactl list short sources`. Ces outils sont
   optionnels et ne sont pas installés par Hakille.
2. Vérifier que le flux de capture est dirigé vers le microphone voulu dans le
   mélangeur de la session. Une source « monitor » capture une sortie audio et
   ne constitue pas le microphone physique.
3. `arecord -l` inventorie les entrées ALSA matérielles si `alsa-utils` est
   installé. Une session graphique fonctionnelle peut utiliser des périphériques
   virtuels qui ne figurent pas dans cette liste matérielle.
4. Lancer Hakille sous l'utilisateur de la session audio. Dans un conteneur,
   une VM ou un environnement cloud, vérifier l'exposition du périphérique et
   du service audio : installer `libasound2-dev` permet de compiler, mais ne
   crée pas de microphone. Une entrée absente doit rester une erreur explicite.

## Test physique ciblé

Sur une machine équipée d'un micro autorisé et sélectionné par défaut :

```sh
cargo test --locked -p hakille-audio capture_start_delivers_10ms_blocks_when_device_available -- --ignored --nocapture
```

Ce test ouvre réellement le micro, attend un bloc pendant trois secondes,
arrête la capture et vérifie les erreurs asynchrones. Il **échoue** si le
périphérique ne démarre pas ou si aucun bloc n'arrive ; il ne transforme pas
ces échecs en tests réussis. Il n'écrit aucun enregistrement sur disque.
Une réception de blocs silencieux peut réussir : ce test valide le transport,
pas le niveau de parole ni la reconnaissance vocale. Les tests ordinaires
laissent ce test physique explicitement ignoré.

Pour une validation complète, vérifier ensuite le niveau avec les réglages du
système, prononcer le mot d'activation dans le flux de l'application, vérifier
la transcription observée et le changement d'état, puis débrancher ou couper
le micro. L'interface doit afficher l'erreur ou l'absence de données, sans
continuer à annoncer une écoute fonctionnelle. Répéter après redémarrage et
après changement de périphérique. Consigner l'OS, le nom du micro, sa
configuration et les résultats ; une compilation Linux ne valide pas le
matériel Windows ou macOS.

## Contrat pour le contrôleur voix

- `input_device_diagnostics()` expose `name`, `is_default`, `config` ou `error`,
  sans ouvrir le flux. `config_info()` décrit la configuration sélectionnée.
- `start()` signale les erreurs de construction et de démarrage. Son succès
  signifie que le pilote a accepté le démarrage, sans preuve de signal reçu.
- `status()` expose les nombres de callbacks, de blocs livrés et d'erreurs,
  l'âge du dernier callback et `failed`. Les compteurs sont des observations
  concurrentes ; ils ne forment pas une mesure de latence bout en bout.
- `take_error()` consomme la première erreur en attente. Le stockage est borné
  à une erreur, tandis que le compteur conserve le nombre d'erreurs signalées.
  Consommer cette erreur ne remet pas `failed` à faux. Seul un nouveau démarrage
  crée un nouvel état ; `is_running()` reste faux après erreur.
- Après une erreur CPAL ou des données invalides, aucun nouveau callback audio
  n'est traité. Le contrôleur doit appeler `stop()` avant de proposer une reprise.
  L'arrêt détruit directement le flux CPAL ; il ne dépend pas d'une pause
  matérielle, absente de certains périphériques ALSA.
- Le callback doit utiliser une **file bornée et un envoi non bloquant**.
  Le worker effectue l'inférence hors du thread audio, surveille les pertes,
  impose un délai d'absence de blocs et remonte ces situations à l'interface.
  Une absence de callback n'entraîne pas toujours une erreur CPAL.

Le rééchantillonnage actuel est linéaire, sans filtre anti-repliement dédié.
Les tests vérifient la continuité entre callbacks et la taille des blocs ;
ils ne constituent pas une mesure de qualité acoustique ni de précision ASR.
