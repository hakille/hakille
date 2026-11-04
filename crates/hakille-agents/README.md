# Exécution des agents Hakille

`AgentSandbox` compile et exécute réellement les deux agents de référence
`echo-agent` et `count-agent`. Les cinq agents métier sous `agents/` restent
des manifests déclaratifs : leur présence dans le registre ne signifie pas
qu'une action métier a été exécutée.

## Contrat du runtime AEl

- Aucun import hôte, WASI, réseau ou fichier n'est lié.
- Le module exporte une mémoire **wasm32 non partagée** et
  `run(i32, i32) -> (i32, i32)`.
- Chaque exécution crée un nouveau store ; son fuel, son échéance et sa
  limite de mémoire linéaire sont définis par `RunLimits`.
- Les fonctions WASM `start` sont soumises aux mêmes budgets que `run`.
  Leur épuisement produit les erreurs explicites `FuelExhausted` ou `Timeout`.
- Au plus quatre tables sont autorisées, chacune limitée à **16 384 entrées**,
  à l'instanciation comme lors d'un `table.grow`. Un dépassement refusé reste
  une erreur même si le module ignore le résultat négatif de la croissance.
- Chaque document JSON d'entrée ou de sortie est plafonné à **1 Mio**.
  L'entrée est validée avant l'instanciation et donc avant le `start` du guest.
  La plage de sortie est vérifiée avant copie et parsing.
- Les erreurs JSON exposent une position, sans recopier le contenu du document.

Ces quotas concernent les ressources guest et les documents échangés. Ils ne
constituent pas une limite globale de mémoire du processus hôte : la compilation
JIT et les métadonnées du moteur nécessitent aussi des ressources. L'autorisation
des actions métier reste à appliquer lors de l'intégration de futurs imports.

## Vérification

Depuis la racine du dépôt :

```bash
cargo test --locked -p hakille-agents wasm_runner
```

Les tests exécutent des modules WAT réels : boucles infinies, mémoire et tables
trop grandes, entrées invalides, sorties hors bornes et JSON à la limite du quota.
