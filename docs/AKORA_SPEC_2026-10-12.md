# Akora — Spécification de design et d'interface
*Hakille · Version 0.1 · 12 octobre 2026 · Référence unique pour le design, l'interface et la pile technique côté client.*

> Archivé tel que reçu. Implémentation : tokens/ + `annora-design` +
> `annora-ui-protocol` + ADR-008 (iced cible sous réserve des spikes).
> Écarts assumés et tracés : nom de crates `annora-*` conservé (rename
> Day-1 à trancher), Slint conservé comme renderer de travail en attendant
> les spikes S1–S4, quota silence daemon passé à 3/jour (§18), Présence
> 6 états + 21 fils implémentés dans `annora-ui`.

## 1. Principes

1. **L'interface s'efface quand tout va bien.** Pas de fenêtre de chat par défaut. Akora apparaît quand il faut décider ou comprendre, jamais pour être vu.
2. **Calme par défaut.** Aucune animation gratuite, aucune notification de rétention, aucun compteur d'engagement. Si Akora marche bien, on l'ouvre moins.
3. **Preuve avant confiance.** Chaque action montre ce qu'elle fait, ce qu'elle coûte, où vont les données et comment l'annuler.
4. **Une couleur, un sens.** La braise veut dire « ta décision est requise ». Rien d'autre.
5. **Un design, plusieurs canaux.** Lumière, voix, texte, vibration : même information, canal choisi selon le contexte.
6. **Universel dès la première ligne.** Écritures, sens de lecture, handicaps, langues : traités au design, pas ajoutés après.

**Refus explicites :** bulle de chat en écran d'accueil, mascotte, avatar humain, gamification, badges, séries quotidiennes, notifications pour ramener l'utilisateur.

## 2. Identité visuelle

- **Signe :** la Lueur, un point lumineux, posée sous un arc de 21 fils. La kora, instrument des griots, a 21 cordes. Le même motif devient la forme d'onde de la voix d'Akora.
- **Logotype :** « akora » en minuscules, géométrique, semi-bold, approche serrée (−1 %). Le « o » est un cercle parfait.
- **Signature :** « akora · par Hakille » en texte/2, taille Caption.
- **Zone de protection :** 2 × le diamètre de la Lueur. **Tailles minimales :** 16 px (signe seul), 64 px (logotype).
- **Interdits :** dégradés, ombres portées, signe en braise, signe animé en dehors du composant Présence.

## 3. Couleur

Sombre par défaut, clair complet. Les ratios sont vérifiés en CI à chaque changement de token.

| Token | Sombre | Clair | Usage |
| --- | --- | --- | --- |
| surface/0 | #0B0C0F | #F6F4F0 | Fond d'application |
| surface/1 | #121419 | #FFFFFF | Cartes, panneaux |
| surface/2 | #1A1D24 | #EDEAE4 | Champs, éléments surélevés |
| ligne | #262A33 | #DAD6CE | Filets 1 px |
| texte/1 | #ECEEF2 | #14161B | Texte principal (≈ 17:1 / ≈ 16:1) |
| texte/2 | #A3A9B6 | #4A5060 | Secondaire (≈ 8:1 / ≈ 7:1) |
| texte/3 | #7A8190 | #667085 | Légendes (≈ 5:1 / ≈ 4,5:1) |
| lueur | #F4EBDD | #14161B | Présence au repos |
| braise | #FF7A2F | #C2410C | Validation requise, et rien d'autre (≈ 7,5:1 / ≈ 4,7:1) |
| sur-braise | #0B0C0F | #FFFFFF | Texte sur fond braise |

**Règles**

- La braise n'apparaît que sur : le bouton de validation, la progression d'un appui long, l'anneau « décision requise ». Jamais en décoration.
- Pas de rouge/vert sémantiques. L'état passe par une icône **et** un mot, jamais par la couleur seule. Les différences (journal, diff d'apprentissage) utilisent des glyphes +/− et un soulignement.
- Le focus clavier est un anneau de 2 px en texte/1, pas en braise.
- Le « verre » (flou 24 px, teinte surface/1 à 72 %) est réservé à la ligne de commande. Repli en aplat si le flou est indisponible ou si « réduire la transparence » est actif.

## 4. Typographie

- **Interface :** Inter Variable, embarquée.
- **Code, empreintes, journal technique :** JetBrains Mono (OFL).
- **Couverture des écritures :** chaîne de repli Noto incluant arabe, devanāgarī, CJK, **Adlam** (écriture du fulfulde) et **N'Ko** (écriture mandingue). Aucun « tofu » toléré : un test CI vérifie que chaque langue livrée s'affiche sans glyphe manquant.

| Style | Taille / interligne | Graisse | Usage |
| --- | --- | --- | --- |
| Display | 40 / 44 | 600 | Titres d'onboarding |
| Title | 28 / 34 | 600 | Titres d'écran |
| Heading | 20 / 26 | 600 | Sections, cartes |
| Body | 16 / 24 | 400 | Texte courant |
| Body-strong | 16 / 24 | 500 | Verbes d'action |
| Caption | 13 / 18 | 400 | Métadonnées |
| Mono | 13 / 20 | 400 | Empreintes, coûts |

Lignes de 68 caractères maximum. Chiffres tabulaires pour coûts et horodatages. Mise à l'échelle utilisateur de 80 % à 200 % sans coupure ni chevauchement. Pas de majuscules au-delà des étiquettes de 12 px.

## 5. Grille, forme, matière

- **Grille de base :** 4 pt. **Espacements :** 4, 8, 12, 16, 24, 32, 48, 64.
- **Rayons :** 8 (contrôles), 16 (cartes, ligne de commande), pilule (étiquettes).
- **Élévation :** par la teinte de surface et un filet de 1 px. Aucune ombre en thème sombre.
- **Icônes :** jeu maison, trait 1,5 px sur grille 24, dessiné en SVG puis compilé en chemins vectoriels au build. Zéro bitmap.
- **Zones tactiles :** 44 × 44 pt (iOS), 48 × 48 dp (Android), 32 × 32 px minimum au pointeur.

## 6. Mouvement, son, haptique

**Durées :** 120 / 200 / 320 / 600 ms. **Entrée :** cubic-bezier(0.2, 0, 0, 1). **Sortie :** cubic-bezier(0.3, 0, 0.8, 0.15). **Ressort (déplacement de cartes) :** raideur 380, amortissement 32, masse 1. Tous ces nombres sont des tokens, jamais écrits en dur.

**Son.** Trois signaux, synthétisés par corde pincée (algorithme Karplus-Strong) : *éveil* (deux notes montantes, 180 ms), *fait* (une note douce, 220 ms), *attention* (deux notes, 320 ms). Aucun fichier audio embarqué. Le volume suit le bruit ambiant et le Sélecteur de Canal.

**Haptique.** *Tick* de 10 ms = validé. *Double tap* = terminé. *Long-court* = attention.

**Règle de redondance.** Tout changement d'état passe par au moins deux canaux (visuel + son **ou** haptique). Le Sélecteur de Canal choisit lesquels selon le contexte (réunion, public, voiture, silence).

**Mouvement réduit.** Les pulsations deviennent des changements d'opacité par paliers. Aucun changement d'échelle, aucun défilement automatique.

## 7. La Présence — composant signature

Un point, quelques états. C'est l'interface par défaut d'Akora.

| État | Visuel | Rythme | Son | Haptique | Coût GPU |
| --- | --- | --- | --- | --- | --- |
| Veille | Point 6 px, lueur à 40 % | Statique | — | — | 0 image/s |
| Écoute | Point 8 px qui respire | 4,0 s, opacité 60→100 %, échelle 1→1,15 | Éveil | Tick | Faible |
| Action | Anneau fin autour du point | Pulsation 1,2 s | — | — | Faible |
| Parole | **21 fils** verticaux | Amplitude = enveloppe audio, 60 i/s, 24 px max | (voix) | — | Moyen |
| Décision requise | Anneau braise fixe | Sans pulsation | Attention | Double tap | Faible |
| Urgence | Double impulsion braise | 0,8 s | Attention | Long-court | Faible |

- **Interruption (barge-in) :** quand l'utilisateur parle par-dessus, les 21 fils retombent en moins de 80 ms et le point passe en Écoute.
- **Petites tailles :** sous 24 px, les 21 fils se réduisent à 7.
- **Silence :** 600 ms après la dernière parole, les fils se replient en point.

| Plateforme | Emplacement de la Présence | Limite connue |
| --- | --- | --- |
| macOS | Icône de barre de menus + ligne de commande en surimpression | — |
| Windows | Zone de notification + surimpression | — |
| Linux / Redox | Surimpression (layer-shell sous Wayland, Orbital sous Redox) | À valider (spike 2) |
| iOS | Live Activity / Dynamic Island, widget d'écran verrouillé, raccourcis | Arrière-plan limité par l'OS : l'« ombre » cloud assure la continuité |
| Android | Tuile rapide + notification persistante minimale | Restrictions batterie selon constructeur |
| Web | PWA : ligne de commande + journal en lecture | Pas d'écoute ambiante hors onglet |
| Hub | Anneau lumineux | — |
| Compagnon | LED + haptique | — |

## 8. Écrans et composants

**Navigation : quatre destinations, pas une de plus.** Journal · Mémoire · Pouvoirs · Réglages.

**Ligne de commande.** Raccourci global configurable (par défaut ⌃⌥Espace). 640 × 56 pt, haut-centre, rayon 16, verre. Texte, voix ou les deux. Résultat inline sur une ligne, `Échap` ferme. Aucun historique de conversation affiché : la trace vit dans le Journal.

**Carte d'action** (proposition ou confirmation), dans cet ordre :

1. Verbe + objet : « Envoyer ce mail à Awa ».
2. Une phrase de raison, avec un lien « Pourquoi ? » qui ouvre la trace (faits du graphe + raisonnement).
3. Trois pastilles : **coût** (en direct), **données** (« Local » ou « Cloud : nom du fournisseur »), **réversibilité** (annulable / définitif).
4. Actions : **Valider** (braise), Modifier, Refuser.

Action irréversible : appui long de 600 ms sur Valider, anneau de progression braise, tick au début et à la fin. Pas de double confirmation par fenêtre modale.

**Journal (écran principal).** Frise groupée par jour. Chaque entrée : heure, verbe + objet, acteur (Akora ou nom du skill), pastille données, coût. Dépliable : trace du pourquoi, bouton **Annuler** en un geste. Recherche sémantique locale, filtres, export. C'est aussi là que s'affichent les diffs : « ce que j'ai appris cette semaine, tu valides ? ».

**Pouvoirs.** Les capacités groupées (Voir, Entendre, Parler, Agir, Dépenser, Partager), chacune avec périmètre, jauge de quota, dernier usage, expiration, interrupteur. Invités temporaires. Bouton « Tout couper ». Registre de consentement en frise.

**Mémoire.** Explorateur du graphe : recherche, fiche par entité (Personne, Événement, Préférence, Fait, Projet, Objectif, Tâche), indicateur de fraîcheur, modifier, oublier. Boîte « contradictions » (« deux choses ne vont pas ensemble »). Bascule des contextes pro/perso. Export au format ouvert.

**Réglages.** Fournisseurs (OpenAI, Anthropic, Ollama, personnalisé) avec pastille « où vont mes données » ; voix (plusieurs voix utilitaires, aucune « star ») ; canaux ; langues ; accessibilité ; sauvegarde ; profils du foyer ; mode simulation.

**Onboarding : 5 étapes, moins de 3 minutes.**

1. Le manifeste d'abord : ce qu'Akora peut et ne peut pas faire sur cet appareil.
2. Choix du fournisseur, avec la pastille « où vont mes données ».
3. Enrôlement voix et visage, local : le gabarit reste dans l'enclave matérielle, avec détection de vivacité.
4. Mode simulation activé par défaut : rien ne s'exécute, tout se prévisualise.
5. Premier « Dis Akora » guidé.

## 9. Écriture

Phrases courtes, verbes d'action, tutoiement par défaut en français (configurable tu/vous, et équivalents selon la langue). Pas d'émojis, pas de « En tant qu'IA… », pas d'excuses de politesse.

- Oui : « Je peux envoyer ce mail à Awa. Rien ne quitte ton appareil. »
- Oui : « Je ne suis pas sûr de la date. Tu peux confirmer ? »
- Oui : « Il y a quelque temps, tu aimais le jazz. C'est toujours le cas ? »
- Non : « Je suis ravi de t'aider ! 😊 »

Le ton exprime l'incertitude réelle : moins sûr, donc plus de nuances ; jamais la même assurance qu'Akora ait raison ou tort.

## 10. Architecture technique du client

Le cœur est 100 % Rust. L'interface en est une projection : elle ne contient aucune logique métier et ne détient jamais de secret.

```
Cœur Rust (akora-kernel, akora-memory, akora-audit…)
        ⇅  akora-ui-protocol (messages typés, versionnés)
   ┌────────────────────────────┬──────────────────────────────────┐
   │ iOS : SwiftUI + UniFFI     │ Bureau + Web : akora-ui          │
   │ Android : Compose + UniFFI │ (iced + wgpu) → Redox, Linux,    │
   │                            │ macOS, Windows, WASM             │
   └────────────────────────────┴──────────────────────────────────┘
```

| Couche | Choix | Pourquoi |
| --- | --- | --- |
| Rendu | wgpu | Vulkan, Metal, DX12 et WebGPU depuis un seul code Rust |
| Toolkit bureau/web | iced | Architecture Elm : l'état est la projection d'un flux de messages, comme le noyau et le journal d'audit |
| Texte | cosmic-text | Mise en forme des écritures complexes, bidirectionnel, repli de polices |
| Accessibilité | AccessKit | Un arbre d'accessibilité unique vers VoiceOver, NVDA, Orca |
| Internationalisation | Fluent + ICU4X | Pluriels et genres, calendriers, formats, tri ; données compilées dans le binaire |
| Audio | cpal + DSP maison | Entrée/sortie portables, sons synthétisés |
| Mobile | SwiftUI + Compose + UniFFI | Les intégrations natives (Live Activity, tuiles, raccourcis) l'exigent ; toute la logique reste en Rust derrière |
| Protocole UI ↔ noyau | akora-ui-protocol (serde + postcard) | Messages typés, même schéma pour natif, UniFFI et WASM |
| Tokens | TOML → xtask → Rust, Swift, Kotlin, CSS | Source unique ; la CI échoue si un client dérive |
| Tests | Captures wgpu sans écran, snapshots de l'arbre d'accessibilité, proptest sur l'état | Régressions visuelles et d'accessibilité en CI |

**Écartés, et pourquoi.** *Slint* : à trancher juridiquement. *Tauri* : vue web, contredit le tout-Rust. *Qt Bridge for Rust* : en bêta publique depuis l'été 2026, dépendance lourde à valider sur Redox. *Flutter* : Dart. *Xilem* (Vello + AccessKit) : meilleur alignement de design mais encore expérimental. On le suit et on y contribue ; c'est le plan B si iced bloque.

**Spikes de la Phase 0 : hypothèses à prouver avant de s'engager.**

| Spike | Question | Critère de sortie | Plan B |
| --- | --- | --- | --- |
| 1. Accessibilité | iced + AccessKit tient-il sur macOS, Windows, Linux ? | Trois lecteurs d'écran parcourent Carte d'action et Journal | Xilem / Masonry |
| 2. Redox | Surimpression + raccourci global sous Orbital ? | Ligne de commande ouverte en ≤ 100 ms | Shell Rust minimal sur framebuffer |
| 3. Saisie composée | IME chinois, japonais, coréen, arabe dans la ligne de commande ? | Saisie correcte dans ces quatre langues | Champ natif par plateforme |
| 4. Rendu événementiel | Zéro image en Veille ? | 0 réveil/s mesuré | Boucle de rendu suspendue par l'OS |

## 11. Performance et énergie (objectifs, mesurés en CI)

| Mesure | Objectif |
| --- | --- |
| Veille | 0 image/s, aucun réveil périodique |
| Raccourci → ligne de commande visible | ≤ 100 ms (p95) |
| Mot de réveil → retour lumineux et sonore | ≤ 250 ms |
| Fin de parole → premier son (modèles locaux) | p50 ≤ 800 ms, p95 ≤ 1,5 s |
| Journal | 120 i/s au défilement, 100 000 entrées (liste virtualisée) |
| Démarrage à froid | ≤ 800 ms bureau, ≤ 1,2 s mobile |
| Mémoire de l'interface | ≤ 80 Mo bureau (hors modèles) |

**Budget de latence, fin de parole → premier son :** détection de fin 150 ms, transcription finale 100 ms, premier jeton System 1 250 ms, premier échantillon vocal 150 ms, lecture 50 ms : **700 ms**. Chaque étage a son test de non-régression.

**Énergie.** Rendu piloté par les événements, suspendu quand la fenêtre est masquée, animations ambiantes plafonnées à 30 i/s sur batterie, tâches non urgentes décalées selon l'intensité carbone du réseau quand elle est connue.

## 12. Accessibilité et international

Non négociable, vérifié avant chaque release.

- **Niveau :** WCAG 2.2 AA au minimum. Parcours complet au clavier. Anneau de focus visible partout.
- **Lecteurs d'écran :** VoiceOver, TalkBack, NVDA, JAWS, Orca via AccessKit ou les API natives.
- **Perception :** échelle de texte 200 %, contraste élevé, mouvement réduit, transparence réduite.
- **Audio :** sous-titres en direct de toute parole d'Akora ; alternative haptique et visuelle à chaque signal sonore.
- **Motricité :** contrôle par bouton unique, utilisation à une main, délais réglables. Point d'entrée prévu pour la langue des signes en vision.
- **Cognition :** mode simplifié (moins d'options, délais plus longs, vocabulaire de base).
- **Sens de lecture :** miroir complet en droite-à-gauche. Les icônes directionnelles se retournent ; la forme d'onde ne se retourne pas.
- **Langues :** Fluent pour les messages, ICU4X pour calendriers, nombres, ordre des noms. Budget d'expansion du texte de 40 %. **Pseudo-localisation en CI** pour attraper chaînes codées en dur et coupures.
- **Écritures :** Adlam et N'Ko prises en charge dès la première langue africaine livrée.

## 13. Matériel : hub et compagnon

- **Aucun écran.** Un anneau lumineux, la Présence en objet.
- **Interrupteur mécanique** micro/caméra avec obturateur physique : il coupe l'alimentation, ce n'est pas un réglage logiciel.
- **LED d'état câblée en dur** : le logiciel ne peut pas l'éteindre quand micro ou caméra sont alimentés.
- **Réparable :** batterie remplaçable sans outil propriétaire, schémas publiés.

## 14. Qualité, recherche, gouvernance du design

- **Design system ouvert :** tokens et composants versionnés en semver, changelog public, décisions de design consignées en ADR.
- **Porte de release :** audit d'accessibilité, ratios de contraste, régression visuelle, budgets de performance, pseudo-localisation. Un seul échec bloque.
- **Recherche utilisateur :** panel incluant des personnes en situation de handicap dès la Phase 1, tests dans au moins cinq langues, suivi longitudinal de quatre semaines (la confiance se construit dans la durée).
- **Indicateurs de design :** temps pour accomplir une tâche, taux de correction après action autonome, taux d'acceptation des propositions, progression de la jauge de confiance. **Jamais** : temps passé, ouvertures quotidiennes, rétention.

## 15. Feuille de route design et interface

- **Phase 0 :** tokens, Présence, ligne de commande (macOS, Linux), les quatre spikes.
- **Phase 1 :** Journal, Cartes d'action, Pouvoirs, onboarding, coques mobiles iOS et Android.
- **Phase 2 :** explorateur de Mémoire, profils du foyer, audit d'accessibilité externe publié.
- **Phase 3 :** hub et compagnon, shell natif Redox.

## 16. Risques et questions ouvertes

- **Maturité de l'écosystème d'interfaces Rust :** accessibilité et saisie composée sont les deux points durs. Les spikes 1 et 3 décident ; Xilem est le plan B.
- **Les 21 fils :** lisibilité aux petites tailles à confirmer en test utilisateur, d'où le repli à 7.
- **Mobile :** la Présence ambiante est bornée par les OS. Le design assume un mode Invité honnête plutôt que de prétendre à l'équivalence.
- **Flou en verre :** coût sur matériel modeste ; le repli en aplat doit être indiscernable côté fonctionnalités.

## 17. Chemin de confiance : sécurité de l'interface

Ce que l'utilisateur voit au moment de décider doit être impossible à falsifier par un skill, une page web ou un document.

- **Rendu de confiance.** Cartes d'action, anneau braise et jauges de consentement sont dessinés par le shell Akora, jamais par un skill. Un skill fournit des *données* (verbe, objet, paramètres) ; le shell choisit gabarit, couleurs et typographie. Aucun markup, image ou couleur fournis par un skill n'atteint une carte.
- **Interface sans pouvoir.** Le processus d'interface tourne sans aucune capacité : il ne parle au noyau que par `akora-ui-protocol`. Le noyau revérifie tout (durée de l'appui long, fraîcheur biométrique, quotas). L'interface propose, le noyau décide.
- **Bande de provenance.** Chaque carte porte une bande non éditable : *Akora*, *skill vérifié (éditeur + nom)* ou *contenu externe*. Un nom de skill trop proche d'un skill déjà installé déclenche un avertissement.
- **Contenu externe marqué.** Tout texte venu d'un mail, d'une page ou d'un fichier s'affiche avec un filet latéral de 2 px et la pastille « externe ». Il ne peut ni déclencher une action ni être lu comme une instruction. Une voix entendue dans une vidéo ou une conversation tierce n'est jamais une commande.
- **Validation hors bande pour le sensible.** Paiement, partage de mémoire, nouveau pouvoir : la voix seule ne suffit pas. Il faut un geste sur l'appareil (appui long) ou une biométrie fraîche (score élevé + vivacité). L'interface propose, le noyau décide.
- **Signal d'attention sécurisé.** Un geste réservé ouvre toujours Journal et Pouvoirs du shell, même sous une surimpression tierce : appui de 2 s sur l'anneau du hub, combinaison de touches réservée sur bureau. C'est le réflexe « qui demande quoi ? ».
- **Écran et presse-papiers.** Les écrans sensibles (Mémoire, wallet) demandent à l'OS d'être exclus des captures quand il le permet. Le presse-papiers n'est jamais lu en arrière-plan ; un collage passe par une carte avec provenance ; les données sensibles copiées expirent après 60 s.
- **Verrouillage.** Après inactivité, Pouvoirs et Mémoire redemandent une authentification. Le Journal reste lisible, contenu masqué.

## 18. Niveaux d'interruption : le budget de silence, côté interface

| Niveau | Canal | Exemple | Coût |
| --- | --- | --- | --- |
| N0 Silencieux | Journal seulement | Tâche de routine terminée | 0 |
| N1 Lueur | La Présence change d'état, rien d'autre | Une proposition à faible valeur attend | 0 |
| N2 Discret | Haptique ou son bref | Réponse à une demande directe, action terminée | 0 |
| N3 Voix | Akora prend la parole | Suggestion proactive à haute valeur | 1 crédit |
| N4 Urgence | Braise + son + haptique long-court | Alarme ou sécurité définies par l'utilisateur | 0, revue dans le Journal |

**Règles**

- Une réponse à une demande directe ne coûte rien. Une proposition *inférée* coûte un crédit. Défaut : 3 crédits par jour, à calibrer en test, sans report d'un jour sur l'autre.
- Deux refus d'affilée sur un même type de proposition le désactivent pendant 14 jours.
- Les modes Réunion et Concentration plafonnent à N1. Seul un N4 explicitement autorisé les traverse.
- Chaque interruption a un lien « Pourquoi ce message ? » vers sa trace. Les crédits restants sont visibles dans Réglages.

## 19. États : vide, attente, hors ligne, erreur

Jamais d'écran blanc, jamais de « service indisponible ». Akora dit ce qu'il peut encore faire.

| Situation | Ce que voit l'utilisateur | Comportement |
| --- | --- | --- |
| Journal vide | Une phrase et trois exemples adaptés aux pouvoirs actifs | Aucune illustration, aucune mascotte |
| Chargement < 400 ms | Rien | Pas de spinner |
| 400 ms à 2 s | Squelette de la forme finale | Sans balayage animé |
| Plus de 2 s | Texte honnête (« Je cherche dans ta mémoire… ») et bouton Annuler | Progression réelle si elle est connue |
| Hors ligne | Pastille « Local » ; ce qui exige le réseau est grisé avec sa raison | « Je le ferai à ta reconnexion », file visible dans le Journal |
| Fournisseur injoignable | Bascule proposée vers le suivant dans l'ordre choisi, ou vers le local | Jamais de bascule silencieuse vers un fournisseur qui change où vont les données |
| Authentification incertaine | Demande d'un second facteur | Pas de blocage dur ; la Présence reste en Veille |
| Contradiction ou conflit de synchro | Carte avec les deux versions, dates, sources | Garder A, garder B, ou garder les deux avec leur contexte |
| Quota atteint | Carte : quel pouvoir, quel plafond | Augmenter pour 1 h, ou laisser bloqué |
| Pouvoir dormant | Reconfirmation (« tu as donné cet accès il y a un an ») | Désactivé sans réponse après 30 jours |

**Gabarit d'erreur d'action, toujours dans cet ordre :** ce qui est fait → ce qui n'est pas fait → ce que tu peux faire (Réessayer, Annuler ce qui a été fait). Le code technique reste dans « Détails », copiable.

## 20. La voix d'Akora : design de l'oral

- **Voix.** Plusieurs voix utilitaires, aucune voix de star. Pas de souffle, de rire ou de soupir synthétiques. Vitesse 1,0 par défaut, réglable de 0,7 à 1,6.
- **Longueur.** Deux phrases au maximum à l'oral. Au-delà : un résumé parlé et « Je te mets le détail dans le Journal ». Trois niveaux : Court (défaut), Normal, Détaillé.
- **Structure.** Le résultat d'abord, la nuance ensuite. Ni « Bien sûr ! », ni reprise de la question.
- **Listes.** Trois éléments à l'oral, le reste au Journal.
- **Nombres, dates, noms.** Lus selon la locale (24 h ou 12 h, séparateurs). Épellation d'un nom propre sur demande.
- **Confirmation vocale.** Reformuler l'action en une phrase et attendre un « oui ». Pour le sensible : carte (section 17).
- **Corrections.** « Non, je voulais dire… » corrige la dernière action (annule, puis rejoue). Ce n'est pas une nouvelle requête.
- **Silence.** Sans réponse après 8 s à une question, Akora n'insiste pas : la proposition passe en N0.
- **Interruption.** Akora s'arrête net et ne reprend que sur « continue ».
- **Langues.** Détection par segment. Réponse dans la langue de la dernière requête, sauf préférence contraire. Phrase de réveil localisée par langue.
- **Prononciation du nom.** « a-KO-ra », accent sur la deuxième syllabe.
- **Tiers présents.** Si d'autres personnes sont détectées, aucun contenu du contexte perso n'est lu à voix haute ; Akora propose le Journal ou des écouteurs.

## 21. Foyer, invités, moments sensibles

- **Reconnaissance visible mais discrète.** Quand Akora identifie qui parle, la Présence affiche l'initiale (un caractère, texte/2) pendant 600 ms. Si le doute subsiste, il ne personnalise pas et le dit : « Je ne sais pas qui parle, je reste général. »
- **Partagé ou privé.** Dans Mémoire, chaque fait porte sa pastille. Un fait partagé n'est jamais fusionné automatiquement, seulement proposé.
- **Invité.** Carte « Autoriser [nom ou une personne] à [action] pendant [durée] ? ». Durée par défaut : 1 h. Un anneau de 1 px se vide dans la Présence ; l'expiration est automatique et entre au registre. Invité temporaire. Bouton « Tout couper ». Registre de consentement en frise.
- **Mineur.** Interface simplifiée, aucun wallet, supervision par catégories d'activité, jamais par le contenu brut. Le mineur voit ce que voit le parent.
- **Détresse.** Seulement sur des mots explicites, jamais sur une simple inflexion de voix. L'écran devient calme : pas de braise, pas d'animation, une phrase simple, un geste pour appeler le contact de confiance choisi à l'avance, un geste pour les numéros d'urgence du pays. Akora ne cherche pas à « gérer » la situation, il aide à joindre quelqu'un. Rien n'est noté, aucun état émotionnel n'est inféré ni stocké.
- **Public.** Bascule en N1 et Journal, contenu masqué, voix refusée sauf demande.
- **Voiture, mains libres.** Voix et haptique seulement. Les actions réversibles se confirment par « oui / non » ; les irréversibles attendent l'arrêt du véhicule.

## 22. Contrats de code : squelettes à valider en Phase 0

Illustrations de la forme visée, non compilées ici. Les valeurs viennent de la section 3.

**Tokens, source unique.**

```toml
# tokens/color.toml
[color.surface.0]
dark  = "#0B0C0F"
light = "#F6F4F0"

[color.braise]
dark  = "#FF7A2F"
light = "#C2410C"
allowed_in = ["validation", "hold-progress", "decision-ring"]
```

Un lint de tokens refuse tout composant qui utilise `braise` hors de `allowed_in`.

**Sortie générée** par `cargo xtask tokens`, jamais éditée à la main.

```rust
pub mod color {
    pub const SURFACE_0_DARK: u32 = 0x0B0C0F;
    pub const SURFACE_0_LIGHT: u32 = 0xF6F4F0;
    pub const BRAISE_DARK: u32 = 0xFF7A2F;
    pub const BRAISE_LIGHT: u32 = 0xC2410C;
}

pub mod motion {
    pub const DURATION_MS: [u16; 4] = [120, 200, 320, 600];
}
```

**Protocole UI ↔ noyau** (`akora-ui-protocol`, versionné).

```rust
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum PresenceState { Idle, Listening, Acting, Speaking, DecisionRequired, Urgent }

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum DataExit { Local, Cloud { provider: String } }

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Provenance {
    Akora,
    VerifiedSkill { publisher: String, name: String },
    External,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Money { pub micros: i64, pub currency: String }

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActionCard {
    pub id: u64,
    pub verb: String,
    pub object: String,
    pub reason: String,
    pub cost: Money,
    pub data_exit: DataExit,
    pub reversible: bool,
    pub provenance: Provenance,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum UiToKernel {
    Approve { card_id: u64 },
    Edit { card_id: u64 },
    Reject { card_id: u64 },
    Undo { journal_id: u64 },
    RevokeAll,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum KernelToUi {
    Presence(PresenceState),
    ShowCard(ActionCard),
    JournalAppended { id: u64 },
}
```

L'interface ne transmet jamais la durée de l'appui long : le noyau la mesure lui-même par le chemin d'entrée de confiance. `Approve` n'est qu'une demande, que le noyau accepte ou refuse.

## 23. Parcours de bout en bout

Quatre scénarios de référence. Ils servent de tests d'acceptation : chaque étape a un canal, un état de Présence et un budget de temps.

**A. « Dis Akora, réponds à Awa que j'arrive à 18 h » : bureau, seul, en ligne, Mail au cran 1**

| # | Ce qui se passe | Présence | Canal | Budget |
| --- | --- | --- | --- | --- |
| 1 | Mot de réveil détecté | Écoute | Lueur + signal d'éveil | ≤ 250 ms |
| 2 | Requête dite, fin de parole détectée | Écoute | — | — |
| 3 | Akora rédige le message et vérifie que le pouvoir Mail est actif | Action | Lueur | ≤ 800 ms |
| 4 | Carte : « Envoyer à Awa : j'arrive à 18 h ». Pastilles : Local, 0 €, annulable 30 s | Décision requise | Anneau braise + voix : « Je lui envoie ça ? » | — |
| 5 | « Oui » ou appui sur Valider | Action | Tick | — |
| 6 | Envoi, entrée au Journal avec sa trace « Pourquoi » | Veille | Signal « fait » | ≤ 1 s |

**B. La même demande en réunion, tiers présents.** Le Sélecteur de Canal passe en N1 et texte : pas de voix, pas de son. La demande se fait au clavier dans la ligne de commande ou par un appui sur la Présence ; la carte s'affiche, Valider se fait au clavier ou au toucher. Rien n'est lu à voix haute. Le Journal garde la trace.

**C. Hors ligne (métro, mode avion).** Les modèles locaux répondent. Le mail ne peut pas partir : la carte dit « Je l'enverrai à ta reconnexion » et la file apparaît dans le Journal. À la reconnexion, un envoi différé de plus de 15 minutes est revalidé avant de partir.

**D. « Akora, laisse Samir utiliser le wifi pendant une heure ».** Carte Invité avec le pouvoir précis et la durée. Quand Samir parle, il est reconnu comme invité non enrôlé : la Présence affiche un anneau de 1 px qui se vide, ses demandes sont limitées au pouvoir délégué, ses corrections ne nourrissent jamais l'apprentissage. À l'expiration, tout se coupe seul et l'événement entre au registre de consentement.

## 24. L'échelle d'autonomie : la confiance se voit et se règle

L'autonomie n'est jamais un réglage global. Elle se règle **par domaine** (Mail, Agenda, Maison, Fichiers, Paiement, Code…) sur cinq crans.

| Cran | Nom | Akora peut | Reste à l'utilisateur |
| --- | --- | --- | --- |
| 0 | Observer | Lire et suggérer | Tout faire |
| 1 | Préparer | Rédiger, organiser, simuler | Valider chaque action |
| 2 | Agir sur le réversible | Exécuter ce qui s'annule (délai d'annulation visible) | Valider l'irréversible et tout ce qui coûte |
| 3 | Agir dans une enveloppe | Exécuter dans un plafond de coût, de volume et de durée | Valider tout ce qui sort de l'enveloppe |
| 4 | Déléguer | Poursuivre un objectif seul, avec rapport périodique | Valider l'irréversible hors enveloppe |

**Règles**

- Chaque domaine démarre au cran 1. Le mode simulation de l'onboarding permet d'essayer les crans suivants sans rien exécuter.
- **La montée est proposée, jamais imposée.** Après un nombre d'actions validées sans correction (par exemple 20 sur 30 jours, à calibrer par domaine), une carte dit ce que ça change et montre les trois derniers cas : « J'ai envoyé 20 mails sans que tu corriges. Je peux envoyer seul les mails annulables ? »
- **La descente est immédiate, en un geste.** Elle est aussi automatique après l'annulation d'une action autonome : le domaine retombe d'un cran, avec l'explication.
- Les crans 3 et 4 exigent une enveloppe visible (plafond, durée) et expirent après 90 jours, avec reconfirmation.
- **Toujours validé, quel que soit le cran :** nouveau pouvoir, partage de mémoire, suppression définitive, dépense hors enveloppe.
- Le noyau garde son propre seuil de confiance : un cran 3 n'autorise pas une action dont la confiance est basse, elle redevient une carte.
- Dans Pouvoirs, une ligne par domaine, cran en cinq segments **et** en toutes lettres : « Cran 2 sur 4, agir sur le réversible ». Cet écran est la vraie jauge de confiance du jour 1, pas une barre abstraite.

## 25. La trace « Pourquoi » : expliquer à trois niveaux

Un lien « Pourquoi ? » existe sur chaque carte, chaque entrée du Journal et chaque interruption. Il ouvre une feuille à trois niveaux, du plus court au plus profond. L'utilisateur s'arrête où il veut.

1. **En une phrase.** « Parce que tu m'as demandé de répondre à Awa, et que ton pouvoir Mail est au cran 1. »
2. **Les faits utilisés.** Les nœuds du graphe mobilisés (Personne : Awa ; Préférence : tutoie Awa ; Événement : dîner à 18 h 30), chacun avec date, source et fraîcheur. Un bouton « Ce fait est faux » ouvre directement la correction.
3. **Le raisonnement.** Le plan exécuté, les pouvoirs invoqués, la confiance (un mot, la valeur en détail), les alternatives écartées, le modèle et le fournisseur qui ont répondu, le coût, la sortie des données, et ce qui aurait changé la décision : « Je t'aurais demandé confirmation si le message avait contenu un montant. »

**Règles**

- La trace est reconstruite à partir de ce qui s'est réellement passé (journal signé), jamais générée après coup.
- Si l'information n'existe pas, la feuille le dit : « Je n'ai pas gardé la raison de ce choix. »
- Le contenu externe cité reste entre filets, marqué « externe ».
- Une trace s'exporte pour un auditeur, sans le contenu personnel qui n'est pas nécessaire à la compréhension.
- Pas de jargon : « confiance basse, moyenne, haute » en surface, la valeur au niveau 3.

## 26. Catalogue de composants et mise en page adaptative

| Composant | Variantes | Règles clés |
| --- | --- | --- |
| Bouton | Primaire (braise), secondaire (contour), tertiaire (texte) | 44 pt de haut (36 px au pointeur). Un seul primaire par vue. Libellé = verbe. |
| Carte d'action | Proposition, confirmation, invité, erreur, expirée | Voir section 8. Jamais plus d'une carte « décision requise » à l'écran. |
| Pastille | Données, coût, réversibilité, externe | Toujours icône + texte. Seule « Données » est cliquable (ouvre le fournisseur). |
| Ligne de liste | Journal, pouvoir, fait | 56 pt minimum. Geste de glissement = Annuler, avec équivalent clavier obligatoire. |
| Interrupteur | — | Libellé à gauche, état dit en toutes lettres. État « verrouillé par politique » distinct de « désactivé ». |
| Jauge de quota | Par pouvoir | Normale, au-delà de 80 %, atteinte. Valeur textuelle toujours visible. |
| Segments d'autonomie | Par domaine | Cinq crans, voir section 24. |
| Feuille | Pourquoi, Détails | Remplace les modales. Se ferme par geste et par Échap. |
| Champ | Texte, vocal | Erreur sous le champ, jamais par la couleur seule. |

**États, pour tous les composants.** Survol : voile de texte/1 à 4 %. Pressé : 8 %. Désactivé : texte/3, avec la raison accessible au focus. Focus : anneau de 2 px en texte/1, décalé de 2 px.

**Politique.** Pas de modale bloquante (sauf ré-authentification). Pas de toasts : tout événement va au Journal ou à la Présence. Pas de pastille numérique de non-lu.

| Classe de largeur | Largeur | Navigation | Journal |
| --- | --- | --- | --- |
| Compacte | < 600 | Barre inférieure, 4 destinations | Liste ; le détail s'ouvre en feuille |
| Moyenne | 600 à 839 | Rail latéral | Liste ; le détail s'ouvre en volet |
| Étendue | ≥ 840 | Rail latéral | Liste et détail côte à côte |

Contenu centré, 720 px de large au maximum, sur grands écrans. La ligne de commande reste à 640 pt, ou pleine largeur moins 32 pt en classe compacte. Les écrans pliables et les fenêtres redimensionnées changent de classe en direct, sans rechargement.

## 27. Appareils modestes et réseaux instables

Akora est conçu pour le monde entier, donc pour des téléphones à 3 Go de RAM, des forfaits de données comptés et des coupures de réseau quotidiennes. Pas pour le seul haut de gamme.

- **Mode léger.** Détecté automatiquement (mémoire, stockage, batterie) et réglable. Modèle local compact, Présence réduite à un état statique et un anneau, 7 fils, pas de verre ni de flou, Journal sans aperçus.
- **Budget de données.** L'interface n'émet aucun appel réseau hors choix du fournisseur. Sur une connexion comptée, la pastille Données affiche aussi le poids estimé (« ≈ 40 Ko ») avant tout appel cloud.
- **Paquets de langue hors ligne.** Voix, transcription et interface par langue, téléchargeables en Wi-Fi, taille affichée, suppression en un geste.
- **Connexion intermittente.** Toute action est mise en file avec reprise sûre (rejouable sans doublon). L'état de la file est visible ; un envoi différé de plus de 15 minutes est revalidé.
- **Voix d'abord.** Tous les parcours critiques se font sans lire : voix, icônes, libellés audio. Pour les personnes peu lectrices ou les langues mal servies par les claviers système, un clavier Adlam et N'Ko est fourni s'il manque.
- **Appareil partagé.** Profils reconnus (section 21), verrouillage rapide, aucune donnée d'un profil visible dans un autre.
- **Cible de référence.** Android d'entrée de gamme, 3 Go de RAM, écran 720p : interface ≤ 60 Mo de RAM et démarrage à froid ≤ 2 s en mode léger, hors modèles.

## 28. Expérimentation sans manipulation

- **Jamais testé en A/B :** écrans de consentement, Pouvoirs, cartes sensibles, demandes de permission, libellés de coût, tout ce qui touche au départ ou à la suppression. Aucune variante n'a pour but d'augmenter les acceptations.
- **Ce qu'on teste :** lisibilité, temps de compréhension, erreurs d'interprétation, auprès de volontaires qui le savent.
- **Télémétrie :** opt-in, locale d'abord, agrégats à confidentialité différentielle si elle est envoyée. Jamais de contenu, d'identifiant stable ni de voix. La liste exacte des indicateurs envoyés est publiée.
- **Interdits de design, vérifiés en revue :** compte à rebours artificiel, bouton « Refuser » plus discret que « Accepter », culpabilisation (« Tu es sûr de vouloir perdre… »), notification de rétention, progression gamifiée.
- **Galerie de composants.** Une application interne, `akora-gallery`, montre chaque composant dans tous ses états, thèmes, langues et tailles de texte. Elle sert aussi de base aux tests de régression visuelle.

## 29. Décisions ouvertes pour Hakille

| Décision | Options | Recommandation |
| --- | --- | --- |
| Raccourci global par défaut | ⌃⌥Espace, ⌥Espace, autre | ⌃⌥Espace, modifiable dès l'onboarding |
| Crédits de proactivité par jour | 1, 3, 5 | 3, ajusté après quatre semaines de panel |
| Voix livrées au lancement | 3 ou 6 par langue | 3 par langue, deux registres : neutre et chaleureux sobre |
| Phrase de réveil par langue | Traduction locale ou « Hey Akora » partout | Mesurer les fausses activations par langue avant de figer |
| Prononciation du nom | « a-KO-ra » | Tester auprès de locuteurs d'au moins huit langues avant verrouillage |
| Toolkit bureau | iced, ou Xilem en plan B | iced, sous réserve des spikes 1 à 3 |
