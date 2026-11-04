# Keystore Hakille

Le trousseau OS reste le backend requis en production. Le helper de repli
`load_or_create_encrypted_file` est réservé au développement et refuse
`HAKILLE_PROD=1` ou `true`, y compris avec un mot de passe correct.

## Repli authentifié v2

Le format est fixe et fait 88 octets : magic `HAKKEY02` (8), version big-endian
`2` (4), sel aléatoire (16), nonce aléatoire (12), seed chiffré (32), tag (16).
Le sel et le nonce viennent du CSPRNG de l'OS. Argon2id version 0x13 dérive la
clé de 32 octets avec 19 Mio, deux passes et une voie. Les paramètres sont
fixés par la version, jamais contrôlés par un fichier non authentifié.
ChaCha20Poly1305 chiffre le seed et authentifie aussi les 40 octets d'en-tête.
Un mot de passe incorrect ou une altération produit une erreur sans seed et
sans réécriture. Les lectures sont bornées à 89 octets pour détecter le surplus.

Le format historique `ANRKEY01` est **refusé et conservé** : son obfuscation XOR
ne permet pas de vérifier le mot de passe ni l'intégrité. Il n'y a pas de
migration automatique. Une migration doit partir d'une sauvegarde de confiance
et traiter les tokens/journaux qui dépendent de l'ancienne clé. Ne pas supprimer
le fichier pour faire disparaître l'erreur : cela recréerait une autre racine.

## Écriture et concurrence

Chaque opération utilise un verrou OS exclusif sur `<fichier>.lock`, partagé
par les processus coopérants. Le fichier de verrou reste sur disque : ne pas
le supprimer lorsqu'un processus peut encore l'utiliser. La création et la
rotation sont sérialisées. Deux premiers démarrages obtiennent le même seed ;
une rotation concurrente ne peut écraser une nouvelle clé avec des identifiants
qui ne correspondent plus au fichier courant.

Les écritures utilisent un fichier temporaire du même répertoire, synchronisé
puis renommé atomiquement. Sur Unix, les nouveaux répertoires sont créés en
0700, les fichiers et verrous en 0600 dès l'ouverture, et les fichiers finaux
ou verrous symboliques sont refusés (`O_NOFOLLOW`). Le répertoire est synchronisé
après publication. Une erreur de synchronisation après renommage est rapportée,
mais le nouveau fichier peut déjà être visible : ne pas supposer un rollback.

Choisir un répertoire parent de confiance appartenant à l'utilisateur. Ces
verrous ne protègent pas contre un acteur capable de remplacer les répertoires
parents ou de supprimer les verrous. Sur Windows, les ACL sont héritées du
répertoire : aucune ACL personnalisée ni garantie DPAPI n'est ajoutée au helper.

`rotate_encrypted_file` **change le seed**, après authentification de l'ancien
mot de passe, puis vérifie le nouveau fichier. Ce n'est pas une simple opération
de changement de mot de passe ; l'ancienne identité cryptographique est remplacée.
Le raccordement de ce helper au démarrage et la migration des consommateurs sont
à vérifier séparément : la présence de cette API ne prouve pas son utilisation.

## Validation

```bash
cargo test --locked -p hakille-keystore
```

Les tests utilisent uniquement des fichiers temporaires et le backend mémoire,
jamais le trousseau réel. Ils couvrent notamment mot de passe incorrect,
corruption, refus du format v1, production interdite, rotation, concurrence,
permissions Unix et absence de seeds dans le `Debug` des conteneurs.
