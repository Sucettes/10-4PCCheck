# PCCheck

Outil de diagnostic portable, sur clé USB, pour vérifier un appareil d'occasion avant de l'acheter. Rien à installer sur la machine analysée : on branche la clé, on lance l'exécutable, on obtient un verdict (Bon achat / À négocier / À éviter) et un rapport HTML + PDF.

## Ce qu'il analyse

| Mode | Contenu |
|---|---|
| **Analyse complète** d'un ordinateur | Inventaire (processeur, mémoire, carte mère, graphique, réseau), disques, batterie, test de charge du processeur (bridage), test mémoire partiel, licence Windows, BitLocker, Secure Boot, TPM, gestion d'entreprise (Intune, Autopilot, Azure AD), tests interactifs (clavier, pixels morts, webcam, micro, haut-parleurs, pavé tactile) et vérifications devant le vendeur |
| **Un seul disque** (SSD, disque dur, clé, disque en boîtier USB) | SMART avec attributs traduits et évalués, vie restante, données écrites, vérifications de cohérence (compteurs remis à zéro ?), auto-tests SMART, scan de surface en lecture seule, test de capacité réelle (fausses clés USB) |
| **Téléphone Android** (par ADB) | Batterie, patch de sécurité, chargeur de démarrage, root, comptes à retirer (verrou FRP), gestion d'entreprise, stockage, liste de vérifications manuelles |
| **Récupération de fichiers** | PhotoRec derrière une interface : source, types de fichiers, destination obligatoirement sur un autre disque |

Chaque donnée affichée a une info-bulle qui explique d'où elle vient et comment la lire.

## Utilisation

Voir `docs/LISEZMOI-CLE.txt` (copié à la racine de la clé). En bref : lancer `windows\PCCheck.exe`, accepter l'invite administrateur (nécessaire pour lire les disques), choisir un mode. Les rapports sont écrits dans `rapports\` sur la clé.

Clé bootable (MemTest86+, Linux de secours) : `docs/CLE-BOOTABLE.md`.

## Construire

Prérequis Windows : Rust (rustup, toolchain MSVC, 1.92 ou plus), Build Tools C++ de Visual Studio, Node 22+.

```powershell
cd app
npm ci
npx tauri build --no-bundle          # target\release\pccheck.exe
cd ..
./tools/fetch-tools-windows.ps1      # adb et PhotoRec (sources officielles, sommes SHA-256)
./tools/assemble-usb.ps1             # dossier de la clé dans dist-usb\
```

`smartctl.exe` (smartmontools 7.5 ou plus) doit être placé dans `tools\windows\` (voir la CI). Linux : `tools/build-smartctl-linux.sh` puis `tools/build-appimage.sh`.

Tests du moteur : `cargo test --workspace --exclude pccheck-app` (l'exécutable de test de l'application exige les droits administrateur à cause de son manifeste).

## Organisation

```
crates/core        disques : smartctl, attributs, cohérence, auto-tests, surface, capacité
crates/inventory   inventaire, batterie, sécurité Windows, charge CPU, test RAM
crates/android     téléphone par ADB
crates/recovery    PhotoRec
crates/report      rapport versionné, seuils, verdict, HTML, PDF (Typst)
crates/assemble    mesures → rapports
app/src-tauri      application Tauri (commandes, tâches longues)
app/src            interface React + TypeScript
docs/PLAN.md       décisions, architecture, feuille de route, limites connues
```

Usage personnel.
