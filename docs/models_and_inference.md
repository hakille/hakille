# Modèles et inférence : références actuelles

Cette ancienne page de conception est remplacée par deux documents maintenus :

- [Modèles locaux et moteurs disponibles](MODELS.md) : les sept identifiants
  exacts du catalogue, leurs fichiers, contrats, runtimes et limites.
- [Exécuter les modèles localement](INFERENCE_LOCALE.md) : installation,
  contrôle SHA-256, tests réels et inférence sur du texte, du WAV ou une image.

La source du catalogue est
[`crates/hakille-inference/src/catalog.rs`](../crates/hakille-inference/src/catalog.rs).
La CLI est implémentée dans
[`crates/hakille-daemon/src/models_cli.rs`](../crates/hakille-daemon/src/models_cli.rs).
Pour UI-TARS, voir aussi le [guide du serveur local](../crates/hakille-perception/UI_TARS_LOCAL.md).

Les anciennes projections d'architecture, tailles de modèles absents,
performances amont et promesses de couverture multiplateforme de cette page
ne décrivaient pas une exécution vérifiée d'Hakille. Elles ne constituent plus
une spécification du produit. Un inventaire ou une inspection de poids reste
distinct d'un forward, et un forward réussi ne mesure pas à lui seul la
qualité métier du modèle.
