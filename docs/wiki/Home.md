# PCCheck

Outil de diagnostic portable, sur clé USB, pour vérifier un appareil **d'occasion avant de l'acheter** : ordinateur, disque, clé USB, téléphone Android. Il récupère aussi des fichiers supprimés.

Chaque analyse se termine par un **verdict** (Bon achat, À négocier, À éviter) et un **rapport** HTML et PDF enregistré sur la clé, avec la raison de chaque point.

## Pages

| Page | Contenu |
|---|---|
| [[Installation]] | Télécharger, préparer la clé, premier lancement |
| [[Analyse complète]] | Inventaire, tests de charge, sécurité, tests interactifs |
| [[Disques]] | Santé SMART, âge, vitesse, liaison USB, scan de surface, capacité réelle |
| [[Téléphone Android]] | Batterie, stockage, sécurité, comptes |
| [[Récupération de fichiers]] | PhotoRec, The Sleuth Kit, TestDisk |
| [[Rapports et verdict]] | Seuils, niveaux, lecture d'un rapport |
| [[Clé bootable]] | MemTest86+ et Linux de secours (Ubuntu) |
| [[Dépannage]] | Problèmes courants |
| [[Développement]] | Construire, tester, publier |

## Principes

- **Lecture seule par défaut.** Seuls le test de vitesse et le test de capacité écrivent, et uniquement un fichier neuf dans l'espace libre, supprimé à la fin.
- **Rien ne sort de la clé.** Aucune connexion Internet n'est nécessaire ni utilisée.
- **Chaque valeur est expliquée** : survole un titre souligné en pointillé dans l'application.
