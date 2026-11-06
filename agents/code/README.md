# Agent Code — fonctions locales

`WorkspaceCodeIndexer` indexe le contenu fourni en mémoire. Son extraction de
symboles est lexicale : elle ne constitue pas un parseur complet du langage.
`CodePatchEngine` prépare un patch et refuse son application en mémoire si le
contenu d'origine a divergé. Il ne lit ni n'écrit automatiquement le dépôt.

Les patches exportés respectent le format unified diff accepté par Git :

- les fins de ligne LF/CRLF et l'absence de saut final sont conservées ;
- les ajouts/suppressions sur contenu vide ont des plages de lignes correctes ;
- un changement nul produit un diff vide ;
- les noms de fichiers sont échappés pour ne pas injecter d'en-têtes ;
- le hunk couvre le fichier entier : il est exact mais pas minimal.

`cargo test --locked -p agent-code` vérifie notamment l'application réelle par
Git dans un répertoire temporaire. Git doit être disponible pour ce test.
Le registre métier demeure déclaratif : ces fonctions locales ne constituent
pas un agent WASM autorisé à modifier les fichiers de l'utilisateur.
