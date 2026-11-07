# Navigation des panneaux desktop

Le journal, la mémoire, le statut, les agents et les réponses longues utilisent
un panneau de consultation paginé. Toutes les entrées **reçues du démon** sont
conservées : l'interface ne coupe plus les listes à douze entrées. La requête
journal/mémoire reste plafonnée à 50 entrées ; parcourir les pages ne télécharge
pas d'autres données.

- `Page précédente` / `Page suivante` déplacent d'une page de huit lignes.
- `Début` / `Fin` atteignent la première ou la dernière page.
- La molette déplace le contenu ; le pied indique la plage de lignes affichée.
- Changer de panneau ou recevoir un nouveau résultat remet la lecture en haut.
- Une fenêtre cachée et une carte de décision ne défilent pas.

Les longues chaînes sans espaces (URL, texte japonais) sont découpées aux
frontières UTF-8, sans perdre leurs caractères. Cela ne constitue pas un moteur
de shaping Unicode complet ni une garantie de segmentation par graphème.

L'arbre AccessKit expose le titre et **tout le contenu** du panneau, y compris
les lignes hors écran, avec des actions de défilement. Les cartes gardent leur
arbre et leur décision séparés. Les tests headless vérifient la navigation,
l'absence de perte de texte, les bornes et l'arbre accessible ; la navigation
Orca, VoiceOver et NVDA sur leurs OS natifs reste à vérifier manuellement.
