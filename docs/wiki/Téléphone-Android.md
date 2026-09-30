# Téléphone Android

Analyse par **adb** (Android Debug Bridge), sans rien installer sur le téléphone.

## Brancher le téléphone

1. Sur le téléphone : *Paramètres > À propos du téléphone*, appuie 7 fois sur **Numéro de build**.
2. *Options pour les développeurs > Débogage USB* : active.
3. Branche le câble (un câble de données, pas seulement de charge) et accepte la clé de l'ordinateur sur le téléphone.

## Ce qui est vérifié

- **Batterie** : santé, cycles quand le téléphone les expose, température.
- **Stockage** : capacité et espace utilisé.
- **Sécurité** : version d'Android, date du correctif de sécurité (à jour s'il a moins de 3 mois, à surveiller jusqu'à 12 mois), signes de root.
- **Comptes encore connectés** : nombre de comptes par type (Google, Samsung...). Un compte Google encore présent bloquera le téléphone après une réinitialisation (protection antivol) : le vendeur doit le retirer.
- **Gestion d'entreprise** : téléphone encore géré par une entreprise.

Les adresses des comptes et l'IMEI ne sont jamais lus : ils ne servent pas au diagnostic.

Une liste de vérifications manuelles (écran, boutons, caméras, IMEI sur la boîte...) est enregistrée dans le rapport.
