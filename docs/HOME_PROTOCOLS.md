# Constellation Domotique Souveraine (`hakille-home`)

> **Contrat d'honnêteté radicale** : zéro cloud constructeur, zéro appareil inventé, zéro succès simulé.
> Chaque protocole implémente un vrai codec filaire Rust pur et des transports réseau/série locaux réels.
> Si aucun appareil réel, pont LAN, adaptateur série ou contrôleur BLE n'est présent, Hakille retourne une
> erreur explicite (`Offline`, `TransportUnavailable`, `LanGuardBlocked`, `ConfirmationRequired`) — jamais
> un faux succès.

---

## 1. Vue d'ensemble architecturale

Le crate [`hakille-home`](../crates/hakille-home) est le noyau domotique, énergétique et spatial de Hakille.
Il unifie **19 protocoles locaux** derrière une interface unique (`SovereignHomeHub`), persistée localement
dans `<data_dir>/home.redb` (ACID, zéro dépendance cloud) et gouvernée par :

1. **`HomeLanGuard` (Frontière zéro-cloud)** : bloque systématiquement les IP publiques WAN, les domaines
   internet publics, les endpoints IMDS cloud (`169.254.169.254`, `169.254.0.0/16`, `fd00:ec2::254`), ainsi
   que les traversées de chemins série (`..`, périphériques non autorisés). Autorise uniquement RFC 1918
   (`10/8`, `172.16/12`, `192.168/16`), loopback, IPv6 ULA (`fc00::/7`) / link-local (`fe80::/10`), multicast
   LAN, `.local` / `.lan` / `.home.arpa`, identifiants `ble://` et périphériques série sûrs (`/dev/ttyUSB*`,
   `/dev/ttyACM*`, `/dev/ttyAMA*`, `/dev/serial/by-id/*`, `COM*`).
2. **`HomeBiscuitPolicy` (Capabilities Biscuit v5 épinglées)** : impose un jeton Biscuit signé par la clé racine
   d'autorité (`CapabilityAuthority`) et distingue les rôles résidents (`Owner`, `AdultResident`, `Minor`,
   `Guest`). Les rôles `Minor` et `Guest` sont refusés par construction sur toute action critique Tier 3.
3. **Maintien physique >= 600 ms (Tier 3)** : toute action physique critique (`UnlockDoor`, `OpenGarage`,
   `DisarmAlarm`, `SetMainBreaker`, `SetWaterValve`, `SetCameraPrivacy`) exige une preuve `PhysicalHoldProof`
   d'au moins `600 ms` (`MIN_HOLD_MS = 600`) ou passe par une `ActionCard` soumise au HUD (maintien Entrée
   600 ms, règle sacrée de la Braise) et journalisée avant/après dans `audit.redb` (Ed25519 + SHA-256).
4. **Reçus cryptographiques filaires (`CommandReceipt`)** : chaque exécution réelle retourne un reçu contenant
   le hash SHA-256 hexadécimal (64 caractères) de la trame effectivement transmise (`frame_sha256`), le tier de
   risque et l'indicateur `executed_on_wire`.
5. **Jumeau Numérique Spatial 3D (`HomeDigitalTwin`)** : modélise les pièces (`RoomZone`) et les poses 3D
   des appareils, supporte le raycasting du regard (`raycast_gaze`), l'interprétation des micro-gestes
   spatiaux (`interpret_micro_gesture`), l'export vers le Palais 3D (`to_palace_3d`) et la projection dans le
   graphe mémoire SPO (`project_to_knowledge_graph`).
6. **Homéostasie Énergétique Souveraine (`EnergyHomeostasisEngine`), Tarifs EDF Tempo & Anomalies** : agrège uniquement la télémétrie
   réellement mesurée (solaire SunSpec, batterie, compteur, PAC, borne VE OCPP). Sans sonde mesurée, elle
   refuse toute spéculation et annonce explicitement `0 appareil mesuré`. Supporte les grilles tarifaires
   locales (`TariffPlan` : fixe, Heures Pleines / Heures Creuses, **EDF Tempo Bleu / Blanc / Rouge**) et la
   détection d'anomalies réelles (`detect_anomalies` : veille anormale, pic de puissance, dérive consommation,
   injection perdue).
7. **Automatisations Réactives Souveraines (`AutomationEngine`)** : évalue des règles déterministes
   locales (`AutomationRule`) sur les états/seuils/présences réels avec anti-rebond (`cooldown_secs`).
   **Invariant non négociable** : une règle ciblant une action Tier 3 (serrure, garage, alarme, disjoncteur,
   vanne d'eau, confidentialité caméra) n'est **jamais** exécutée silencieusement — elle est convertie en
   `AutomationEscalation` exigeant une approbation humaine avec maintien physique `>= 600 ms`.
8. **Résolveur d'Intentions Hors-Ligne FR/EN (`resolve_home_intent`)** : transforme une phrase naturelle
   française ou anglaise (`« éteins les lumières du salon »`, `« ouvre la porte d'entrée »`) en commande
   domotique typée 100 % hors-ligne sans appel cloud ni LLM externe, en conservant le niveau de risque réel.
9. **Synchronisation P2P Chiffrée du Jumeau Numérique (`mesh_sync` sur `hakille-mesh`)** : encode des deltas
   du jumeau spatial (`TwinDeltaPacket`) dans des trames `MeshFrame` chiffrées AEAD et signées Ed25519 sur le
   bus local air-gap (`VirtualAirGapBus`) ; toute falsification ou clé étrangère est rejetée avant fusion.

---

## 2. Matrice des 19 protocoles LAN

| # | Identifiant | Protocole | Transport / Framing | Port par défaut | État réel & limite matérielle honnête |
|---|---|---|---|---|---|
| 1 | `matter` | Matter 1.3 | UDP + TLV + AEAD ChaCha20-Poly1305 (PASE/CASE) | `5540` | Codec TLV & session AEAD vérifiés ; commissioning complet exige un nœud Matter physique sur le LAN. |
| 2 | `thread-coap` | Thread / CoAP (RFC 7252) | UDP CoAP (GET/PUT JSON) | `5683` | Client/serveur UDP CoAP réel vérifié en loopback ; un routeur de bordure Thread (OTBR) est requis pour les nœuds 802.15.4. |
| 3 | `mdns-dnssd` | mDNS / DNS-SD (RFC 6762-6763) | UDP Multicast `224.0.0.251` | `5353` | Sondes PTR/SRV/TXT/A réelles sur socket UDP ; 0 résultat si aucun service LAN ne répond dans la fenêtre. |
| 4 | `ssdp-upnp` | SSDP / UPnP LAN | UDP Multicast `239.255.255.250` | `1900` | M-SEARCH réel et parseur LOCATION/USN/ST ; filtré par `HomeLanGuard`. |
| 5 | `mqtt` | MQTT v3.1.1 & v5.0 | TCP / TLS natif | `1883` | Handshake `CONNECT`/`CONNACK`, `PUBLISH`, `DISCONNECT` réels testés en loopback TCP et via IPC démon. |
| 6 | `zigbee` | Zigbee (ZCL / ASH-EZSP / ZNP / Z2M) | Série local ou pont MQTT LAN (Zigbee2MQTT) | Série / MQTT | Codecs ZCL, ASH-EZSP (CRC-16 CCITT), ZNP et dispatch réel Zigbee2MQTT (`zigbee2mqtt/<id>/set`) vérifiés ; le bus radio exige un dongle USB ou pont Zigbee2MQTT local. |
| 7 | `zwave` | Z-Wave SerialAPI | Série local (SOF + LRC) | Série | Framing SerialAPI + LRC vérifié ; exige un contrôleur USB Z-Wave physique. |
| 8 | `ble-gatt` | BLE GATT / BTHome v2 / Ruuvi / MiBeacon | Bluetooth Low Energy local | BLE | Décodeurs BTHome v2, RuuviTag RAWv2 et MiBeacon réels ; le scan radio exige un adaptateur BLE OS actif. |
| 9 | `knx-ip` | KNXnet/IP (cEMI / DPT) | UDP multicast/unicast | `3671` | Encodage/décodage cEMI, adresses de groupe 3 niveaux et DPT1/DPT5/DPT9 vérifiés ; exige une passerelle KNXnet/IP locale. |
| 10 | `modbus-tcp` | Modbus TCP (MBAP) | TCP | `502` | Trames MBAP lecture/écriture registres et bobines vérifiées sur TCP loopback ; exige un automate/compteur Modbus LAN. |
| 11 | `modbus-rtu` | Modbus RTU (RS-485) | Série RS-485 (CRC-16 Modbus) | Série | Trames RTU + CRC-16 vérifiés ; exige un adaptateur USB-RS485 physique. |
| 12 | `homekit-hap` | Apple HomeKit HAP | TCP HTTP + ChaCha20-Poly1305 | `51826` | Blocs AEAD HAP (`hap_encrypt_block`/`hap_decrypt_block`) et caractéristiques JSON vérifiés ; exige un accessoire HAP appairé. |
| 13 | `esphome` | ESPHome Native API | TCP (trame plaintext + varint Protobuf) | `6053` | Framing varint + messages Protobuf natifs vérifiés ; exige un nœud ESPHome sur le LAN. |
| 14 | `home-assistant` | Home Assistant Local WS/REST | WebSocket RFC 6455 pur Rust & HTTP local | `8123` | Client WebSocket natif (`WsClient`) + dispatch réel `call_service` (attributs `ha_token`, `entity_id`) vérifiés en loopback ; exige une instance HA locale + jeton long-terme. |
| 15 | `philips-hue` | Philips Hue CLIP v2 | HTTP/HTTPS local | `443` | Dispatch HTTP CLIP v2 réel (`send_hue_clip_v2_http`, en-tête `hue-application-key`, `hue_rid`) vérifié en loopback ; exige un pont Hue local et clé appairée par bouton physique. |
| 16 | `shelly-rpc` | Shelly Gen2/Gen3 JSON-RPC | HTTP local (`/rpc`) | `80` | Appels JSON-RPC réels vérifiés sur serveur HTTP loopback ; exige un module Shelly sur le LAN. |
| 17 | `ocpp-ev` | OCPP 1.6J / 2.0.1 (Borne VE) | WebSocket RFC 6455 pur Rust | `8887` | Enveloppes `[2, uid, action, payload]` (`RemoteStartTransaction`, `SetChargingProfile`, etc.) vérifiées sur WS loopback ; exige une borne VE compatible OCPP. |
| 18 | `sunspec-solar` | SunSpec Solaire & Batterie | Modbus TCP (modèles 101/103/124) | `502` | Décodage SunSpec échelle/registres vérifié ; exige un onduleur/batterie SunSpec sur le LAN. |
| 19 | `onvif-rtsp` | ONVIF WS-Discovery / RTSP | UDP Multicast `239.255.255.250` + RTSP | `3702` | Sondes SOAP WS-Discovery + dispatch RTSP `DESCRIBE`/`OPTIONS` sur TCP loopback vérifiés ; exige une caméra IP locale ONVIF. |

---

## 3. Commandes REPL, IPC, HUD Desktop (`F5`) et Passerelle Mobile (`hakille-mobile`)

### REPL `hakille-daemon`
- `:home` — Liste les pièces, appareils réellement appairés et reçus cryptographiques persistés dans `home.redb`.
- `:home-protocols` — Affiche le catalogue des 19 protocoles avec transports, ports et notes d'honnêteté.
- `:home-discover [protocole]` — Lance une découverte LAN active réelle (mDNS 5353, SSDP 1900, ONVIF 3702).
- `:home-room <id> <nom> [étage] [x y z]` — Enregistre une pièce dans le jumeau numérique 3D.
- `:home-pair <id> <nom> <pièce> <protocole> <catégorie> <endpoint_lan>` — Appaire un appareil après validation `HomeLanGuard` et journalisation `HomeDevicePaired`.
- `:home-cmd <device_id> <action> [json_params] [--hold-600ms]` — Prévisualise (`preview_command`), impose le maintien >= 600 ms sur Tier 3, puis exécute avec jeton Biscuit et audit Ed25519.
- `:home-ask <phrase FR/EN>` — Résout une intention domotique 100 % hors-ligne (`resolve_home_intent`) sans cloud et prévisualise la commande/le risque avant toute action.
- `:home-rules` / `:home-rules-eval` — Liste et évalue les automatisations réactives locales ; toute action Tier 3 est obligatoirement escaladée (`AutomationEscalation`) et jamais exécutée silencieusement.
- `:home-energy` — Affiche le bilan énergétique réel et le verdict de l'arbitre d'homéostasie.
- `:home-tariffs [tempo-bleu|tempo-blanc|tempo-rouge|hphc|fixe]` — Affiche le tarif horaire souverain actif et l'estimation uniquement sur télémétrie réelle.
- `:home-anomalies` — Exécute le détecteur d'anomalies énergétiques sur les échantillons réellement mesurés.
- `:home-mesh-sync` — Sérialise un delta du jumeau numérique en trame AEAD + Ed25519 (`hakille-mesh`) et vérifie le cycle chiffré local.
- `:home-sync-graph` — Projette l'état courant du jumeau numérique dans le graphe mémoire SPO (`hakille-memory`).

### HUD Desktop Natif (`hakille-ui-desktop`) & Protocole IPC (`hakille-ui-protocol`)
- **`F5`** (ou `:home`) : ouvre le panneau **Constellation Domotique** (`PanelKind::Home`), requête `UiToKernel::HomeDevicesRequest`, affiche le bilan énergétique mesuré, les appareils appairés (badge `[CRITIQUE · maintien 600 ms]` en Braise uniquement sur les actionneurs de sécurité Tier 3), le dernier reçu SHA-256 et le catalogue des 19 protocoles.
- **`:home-discover [protocole]`** : émet `UiToKernel::HomeDiscoverRequest`.
- **`:home-cmd <device_id> <action> [json]`** : émet `UiToKernel::HomeCommandRequest`. Si l'action est Tier 3 sans preuve de maintien >= 600 ms, le démon refuse l'exécution directe et diffuse `KernelToUi::ShowCard` (maintien Entrée 600 ms requis).

### Passerelle Mobile (`hakille-mobile` / `MobileBridge` UniFFI)
- Actions exposées : `LoadHomeDevices`, `DiscoverHome`, `SendHomeCommand` (`MutationKind::HomeCommand`), avec état synchronisé dans `MobileSnapshot` (`home_devices`, `home_protocols`, `home_energy`, `last_home_receipt`).
- **Garanties de sécurité** : en mode *Guest Pass*, toute mutation domotique est bloquée localement avant envoi ; toute commande Tier 3 exige un `hold_duration_ms >= 600` vérifié avant émission IPC.

---

## 4. Extensibilité SDK (`.hap`) et CLI `hakille-pkg`

Le SDK standalone [`hakille-sdk`](../../hakille-sdk) expose le module `hakille_sdk::home` et le CLI `hakille-pkg` :
- `HomeProtocolKind::all()` (19 protocoles alignés avec `default_port()` et `from_str_loose()`),
- `format_home_capability` / `validate_home_capability` (`home:<protocole>:<scope>`, wildcards interdits),
- `validate_local_home_endpoint` (garde LAN zéro-cloud / anti-IMDS / anti-traversée série),
- `hakille-pkg list-protocols` — liste les 19 protocoles souverains et leurs ports par défaut,
- `hakille-pkg check-endpoint <endpoint>` — teste en local le filtrage zéro-cloud `validate_local_home_endpoint`,
- `hakille-pkg init-home <nom> --protocol <protocole>` — génère un squelette d'agent `.hap` domotique pré-configuré avec capability strictement bornée.

---

## 5. Suite de Conformité Formelle (`conformance`)

8 tests de conformité formelle (`home_conformance_01..08` dans `conformance/src/home_protocols_conformance_tests.rs`, intégrés aux **74 tests rapides** de `cargo xtask conformance`) verrouillent l'ensemble des invariants :
- `01` — Couverture intégrale des 19 protocoles & absence de doublon de slug.
- `02` — Garde zéro-cloud `HomeLanGuard` (blocage WAN, IMDS AWS/GCP, traversée série).
- `03` — Maintien physique `>= 600 ms` obligatoire sur Tier 3 + refus `Guest`/`Minor`.
- `04` — Reçus filaires SHA-256 réels (MQTT loopback, Shelly JSON-RPC loopback, Modbus TCP loopback, CoAP loopback).
- `05` — Honnêteté énergétique (0 sonde mesurée ⇒ aucune puissance inventée).
- `06` — Automatisations réactives (escalade Tier 3 obligatoire) & résolveur d'intentions hors-ligne FR/EN.
- `07` — Tarifs EDF Tempo, détecteur d'anomalies et synchronisation P2P chiffrée AEAD + Ed25519 (`hakille-mesh`, rejet de trame altérée).
- `08` — Dispatch filaire réel (Home Assistant WS, Philips Hue CLIP v2, Zigbee2MQTT, ONVIF RTSP) et gouvernance mobile (`MobileBridge` Guest Pass + maintien 600 ms).

