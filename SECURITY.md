# Sécurité

PCCheck s'exécute en administrateur et lit les disques directement : une faille peut avoir des conséquences réelles.

## Signaler une faille

Par un **signalement privé** : onglet *Security* du dépôt, puis *Report a vulnerability*. Jamais dans une issue publique.

Indique la version (nom de la release), le système, et les étapes pour reproduire. Tu recevras une réponse dès que possible ; le correctif est publié dans une nouvelle release, puis le signalement est rendu public.

## Versions suivies

Seule la dernière release est corrigée.

## Principes de conception

- Lecture seule par défaut. Les seules écritures (test de vitesse, test de capacité) créent un fichier neuf dans l'espace libre et le suppriment ; elles sont refusées sur le disque du système et celui de l'outil pour le test de capacité.
- Une récupération de fichiers n'écrit jamais sur le disque source (destination vérifiée sur un autre disque physique).
- Aucun shell n'est lancé : seuls des outils connus (smartctl, adb, PhotoRec, TestDisk, The Sleuth Kit) avec des arguments construits par l'outil.
- Aucune connexion réseau ; les rapports restent sur la clé.
- Outils tiers téléchargés depuis leurs sources officielles, sommes SHA-256 inscrites dans `VERSIONS.txt`.
