# ADR 003: Modèle Mémoriel en Triplets SPO avec redb et Index Vectoriel HNSW

## Statut
Accepté

## Contexte
Le RAG conventionnel (Retrieval-Augmented Generation) sur des morceaux de texte découpés arbitrairement (chunks) présente des limites structurelles :
- Il retrouve des passages textuels bruts mais ne modélise pas les relations logiques entre faits.
- Il souffre d'hallucinations d'agrégation et ne gère pas l'oubli progressif ni la détection de contradictions.
- Les bases de données graphe lourdes (Neo4j, Dgraph) exigent un serveur central Java ou Go incompatible avec un fonctionnement local souverain et léger en Rust.

## Décision
Modéliser la mémoire comme un **graphe de triplets orientés SPO** (Sujet, Prédicat, Objet) enrichi de métadonnées épistémiques (poids, horodatage, source, confiance, partition de contexte).
- **Moteur de stockage local (Tier Edge)** : **redb**, moteur clé-valeur 100% Rust, sans runtime externe, transactions ACID, accès mémoire mmap instantané.
- **Indexation triple** : Indexation ordonnée SPO, POS et OSP pour permettre des traversées bidirectionnelles rapides sans parcours complet de la base.
- **Indexation vectorielle sémantique** : Intégration de l'index HNSW pur Rust **instant-distance** pour les recherches par similarité floue.
- **Décroissance temporelle** : Réduction exponentielle du poids des nœuds non réactivés au fil du temps sans jamais détruire l'historique d'audit.

## Conséquences
- **Positives** : Zéro dépendance externe lourde, temps de réponse sub-milli-seconde sur SSD NVMe, partitionnement natif entre contextes professionnels et personnels, gestion déterministe des contradictions.
- **Négatives / Défis** : Nécessite la sérialisation binaire compacte des nœuds via bincode pour maximiser le débit d'I/O.
