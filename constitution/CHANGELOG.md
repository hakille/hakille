# Changelog de la Constitution d'Hakille

## [1.0.1] - 2026-10-16
### Complété
- Complétion documentaire des 10 règles (`rule-1`..`rule-10`) dans `constitution/v1.0.md` : chaque règle avec ID canonique + enforcement (`HardBlock` sauf `rule-10` en `SoftWarning`).
- `rule-8` clarifiée AV (*continuous AV capture*) : jamais de flux brut hors hôte, traitement local.
- Gouvernance précisée : fondation indépendante, quorum >= 5, revue 30 j + commentaire public obligatoire, semver + diff-puis-validation, kill switch.
- Batterie éthique de référence portée à 12 dilemmes dans la spec (voir v1.0.md Annexes + `docs/constitution_and_ethics.md` §5).
- Sans changer `effective_since` (toujours 2026-03-15) ni la version normative 1.0.0.

## [1.0.0] - 2026-10-04
### Ajouté
- Définition initiale des 4 articles fondamentaux (Primauté humaine, Souveraineté de la mémoire, Cloisonnement Biscuit, Auditabilité).
- Encodage des 10 règles constitutionnelles machine-vérifiables dans `hakille-constitution`.
- Matrice de dilemmes éthiques de conformité.
