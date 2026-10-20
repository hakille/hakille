## Résumé

Décrivez le changement et le problème résolu.

## Type de changement

- [ ] Correction de bug
- [ ] Nouvelle capacité
- [ ] Documentation
- [ ] Refactorisation / sécurité
- [ ] Tests / CI

## Vérifications

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --no-deps`
- [ ] `cargo test --workspace --locked`
- [ ] `cargo run -p xtask -- tokens --check` (si les tokens changent)
- [ ] `cargo run -p xtask -- conformance` (si la conformance change)
- [ ] `cargo deny check bans sources advisories`

## Contrat d'honnêteté et sécurité

- [ ] Aucun stub silencieux, faux succès ou mesure inventée.
- [ ] Les chemins sans modèle/périphérique/permission échouent explicitement
      ou affichent `SKIP`.
- [ ] Aucun fichier `models/`, secret, token ou donnée personnelle ajouté.
- [ ] Les vérifications de capabilities et les frontières de sandbox sont
      préservées.

## Notes

Informations complémentaires, limites connues ou suivi nécessaire.
