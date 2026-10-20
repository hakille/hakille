# ADR 002: Contrôle d'Accès par Capabilities Décentralisées via les Jetons Biscuit

## Statut
Accepté

## Contexte
Les assistants et agents conventionnels reposent généralement sur des permissions statiques déclarées dans un manifeste de type Android (READ_SMS, ACCESS_NETWORK) ou sur des rôles IAM centralisés.
Ces approches sont inadaptées à Hakille pour trois raisons :
1. **Délégation multi-agents** : Un agent principal doit pouvoir créer dynamiquement des sous-tâches déléguées à des sous-agents sans leur transmettre l'ensemble de ses pouvoirs.
2. **Atténuation hors-ligne** : Les décisions doivent être prises sans interroger un serveur d'autorisation centralisé qui créerait un goulot d'étranglement de latence et un point de défaillance unique.
3. **Périmètre temporel et contextuel fin** : Les permissions doivent pouvoir être bornées à un chemin de fichier précis, un montant maximum, une URL donnée et une durée de validité en secondes.

## Décision
Adopter **Biscuit** (`biscuit-auth`) comme format unique et universel de capability pour l'ensemble du système Hakille.
- Chaque jeton Biscuit est signé cryptographiquement par la clé racine du noyau.
- L'atténuation est garantie mathématiquement par construction (moteur Datalog embarqué) : un jeton atténué ne peut que restreindre les droits existants.
- Les blocs du jeton permettent de spécifier des vérifications fines (`caveats`).

## Conséquences
- **Positives** : Preuve formelle de non-escalade de privilèges, vérification instantanée en mémoire en moins de 100 microsecondes, élimination des attaques par usurpation de permissions.
- **Négatives / Défis** : Légère verbosité des signatures par blocs cryptographiques nécessitant une sérialisation compacte.
