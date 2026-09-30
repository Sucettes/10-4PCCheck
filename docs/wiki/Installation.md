# Installation

## Télécharger

Onglet **Releases** du dépôt, dernière version (`v<version>-build.<n>`) :

- **`PCCheck-windows.zip`** : dossier complet de la clé. Contient `PCCheck.exe` et les outils tiers (smartctl, adb, PhotoRec/TestDisk, The Sleuth Kit), téléchargés depuis leurs sources officielles ; sommes SHA-256 dans `windows/tools/VERSIONS.txt`.
- **`PCCheck-linux.AppImage`** : version Linux.

Chaque release est publiée automatiquement quand `master` change et que les tests et les deux builds passent.

## Préparer la clé

1. Décompresse `PCCheck-windows.zip` à la **racine** de la clé (ou dans un dossier `PCCheck/`).
2. Place `PCCheck-linux.AppImage` dans `linux/` sous le nom `PCCheck.AppImage`.

Structure obtenue :

```
windows/      PCCheck.exe + tools/
linux/        PCCheck.AppImage
rapports/     rapports générés (JSON, HTML, PDF)
recup/        destination proposée pour la récupération
LISEZMOI.txt
```

Clé de 16 Go minimum, 32 Go conseillés (rapports, récupérations, et Ubuntu pour la [[Clé bootable]]).

## Premier lancement

**Windows**
1. Ouvre `windows\PCCheck.exe` (pas d'exécution automatique depuis une clé).
2. SmartScreen (« Windows a protégé votre ordinateur ») : *Informations complémentaires*, puis *Exécuter quand même*. L'outil n'est pas signé.
3. Accepte l'invite administrateur : sans elle, ni le SMART ni la lecture directe des disques ne sont possibles. L'application affiche « Droits limités » dans ce cas.

**Linux**
```
chmod +x PCCheck.AppImage
sudo ./PCCheck.AppImage
```
`sudo` est nécessaire pour lire le SMART et les disques.
