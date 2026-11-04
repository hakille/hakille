# Effets natifs des substrats — contrat vérifiable

Ce document décrit le comportement des méthodes `Substrate` après le durcissement
du 3 novembre 2026. Une capability valide autorise une opération ; elle ne prouve
pas qu'un pilote natif existe. Lorsqu'un binding manque, l'appel retourne une
`HakilleError::SubstrateError` explicite après la validation des droits.

## Capture, audio et injection

| Plateforme | Capture d'écran | Injection clavier/souris | Capture et lecture audio via `Substrate` |
| --- | --- | --- | --- |
| Windows | GDI réel conservé ; erreur explicite hors Windows ou échec OS | SendInput/Win32 conservé | Refus : backend non raccordé |
| Linux | Refus : X11/Wayland/DRM non raccordés | Chemin uinput conservé ; refuse si périphérique indisponible | Refus : backend non raccordé |
| macOS | Refus : CoreGraphics non raccordé | Refus : CGEventPost non raccordé | Refus : backend non raccordé |
| Mobile | Refus : MediaProjection/ReplayKit non raccordés | Refus : binding accessibilité non raccordé | Refus : backend non raccordé |
| Web | Refus : getDisplayMedia non raccordé | Refus : binding navigateur non raccordé | Refus : buffer mémoire seul, aucune restitution native |
| Redox | Refus : Orbital non raccordé | Refus : Orbital non raccordé | Refus : schéma audio en mémoire seul, aucune restitution native |

Les champs de manifeste expriment encore des périmètres de capacités et budgets ;
ils ne remplacent pas cette matrice de disponibilité des effets. Une preuve de
fonctionnement du périphérique réel exige un essai sur l'OS concerné.

### Audio

`start_audio_capture` ne crée plus de handle fictif. `play_audio` ne signale plus
un succès après un no-op ou l'ajout dans un buffer mémoire. `stop_audio_capture`
refuse les handles puisqu'aucun flux natif n'est géré par ces six implémentations.
Les variantes `stop_audio_capture_with_cap` Windows/Linux vérifient toujours
`Audio/Read` avant de retourner l'indisponibilité.

Le crate **`hakille-audio` reste inchangé** : il dispose d'une implémentation CPAL
réelle utilisée séparément. Son existence ne signifie pas que les six substrats
gèrent son cycle de vie. Une intégration future doit définir le registre des
flux, leur propriétaire, la livraison des échantillons, la conversion de format,
les limites mémoire et l'arrêt réel ; un simple identifiant ne suffit pas.

### Images de démonstration

`LinuxDisplayCaptureEngine`, `MacosQuartzDisplayCapture` et
`WebScreenCaptureAdapter::capture_frame` restent des générateurs synthétiques
pour tests structurels et démonstrations. Ils sont explicitement documentés comme
tels. Ils ne sont plus appelés par `Substrate::capture_screen`. Les buffers
synthétiques mobile/Redox ont été retirés de cette API. L'ingestion Web
`ingest_image_data` conserve sa fonction de validation de pixels fournis par
l'appelant ; elle n'acquiert pas elle-même un flux navigateur.

### Autorisations

Les vérifications `Audio/Read`, `Audio/Write`, `Sensor/Read` et
`GuiInput/Execute` restent en place. Les nouveaux tests utilisent une autorité
Biscuit épinglée et distinguent le refus de capability de l'indisponibilité du
backend. Les tests d'injection sans binding portent sur macOS/mobile/Web/Redox ;
ils ne déclenchent pas d'événement réel sur Windows/Linux.

## Agents sandboxés

`spawn_sandboxed` ne possède pas de moteur WASM raccordé dans les six substrats.
Après vérification des capabilities déléguées, il retourne une erreur explicite
et ne crée aucun processus ni handle synthétique. Le runtime Wasmtime des agents
de référence dans `hakille-agents` est distinct et reste fonctionnel.

## Vérification

Les tests unitaires des six crates couvrent le refus d'effets indisponibles, la
priorité des contrôles de capabilities, la préservation des helpers de
simulation et le contrat `spawn_sandboxed`. Le test de conformance écran exige
un refus explicite pour les cinq bindings absents et conserve la branche GDI
réelle Windows. L'exécution des tests hors de leur OS cible ne valide pas les
périphériques physiques ni le support natif de cet OS.
