# Clé bootable (phase 6)

Une seule clé sert à deux usages :
- **dans l'OS** du PC analysé : le dossier de l'outil (Windows et Linux), lancé à la main ;
- **au démarrage** : un menu Ventoy pour MemTest86+ (test RAM complet) et un Linux live qui lance l'AppImage de l'outil quand Windows est absent, cassé ou verrouillé.

Ventoy garde une partition de données exFAT normale : on y copie les ISO **et** le dossier de l'outil, visibles depuis Windows comme une clé ordinaire.

## Ce qu'il faut

| Élément | Source | Remarque |
|---|---|---|
| Ventoy | https://www.ventoy.net | Installation sur la clé : **efface la clé** |
| MemTest86+ | https://www.memtest.org (image ISO « Linux ISO ») | Libre (GPL), démarre en BIOS et UEFI |
| Linux live | Ubuntu LTS bureau (https://ubuntu.com/download/desktop) | Défaut proposé par le plan : glibc récente, pilotes graphiques larges, Secure Boot signé |
| Dossier de l'outil | `tools/assemble-usb.ps1` puis `dist-usb/` | Voir le LISEZMOI de la clé |

Taille de clé conseillée : 16 Go minimum (Ubuntu ~6 Go, MemTest86+ ~20 Mo, outil ~100 Mo avec l'AppImage), 32 Go pour garder de la place aux rapports et aux récupérations.

## Procédure (une fois, sur ton PC)

1. **Sauvegarde** ce qui est sur la clé : l'étape 2 efface tout.
2. Lance `Ventoy2Disk.exe`, choisis la clé (vérifie la lettre et la taille deux fois), option **Secure Boot Support** cochée, puis **Install**.
3. La clé réapparaît avec une partition `Ventoy` (exFAT). Copie à sa racine :
   - l'ISO de MemTest86+ ;
   - l'ISO d'Ubuntu ;
   - le contenu de `dist-usb/` dans un dossier `10-4PCCheck/` (voir `tools/assemble-usb.ps1`).
4. Optionnel : `ventoy/ventoy.json` pour renommer les entrées du menu (« Test RAM complet », « Linux de secours »).

## Démarrer sur la clé chez un vendeur

1. Éteins le PC, branche la clé, rallume en appuyant sur la touche du menu de démarrage (F12 Dell/Lenovo, F9 HP, F8 Asus, Échap sur certains portables).
2. **Secure Boot actif** : au premier démarrage sur chaque machine, Ventoy affiche un écran bleu « Verification failed ». Choisis *Enroll key from disk*, puis le fichier `ENROLL_THIS_KEY_IN_MOKMANAGER.cer` de la partition VTOYEFI, *Continue*, *Yes*, *Reboot*. C'est une étape de 1 minute, à faire devant le vendeur ; elle ne modifie pas Windows.
3. Menu Ventoy :
   - **MemTest86+** : laisse tourner au moins un passage complet (15 à 60 min selon la RAM). Une seule erreur = barrette défectueuse.
   - **Ubuntu** : « Essayer Ubuntu », puis ouvre le dossier `10-4PCCheck/linux/` de la partition Ventoy et lance l'AppImage avec `sudo` depuis un terminal pour lire les disques.

## Limites connues

- Certains PC d'entreprise bloquent le démarrage externe par un mot de passe BIOS : dans ce cas, seule l'analyse depuis Windows est possible (et c'est un indice à noter : machine encore gérée par une entreprise ?).
- Macs Intel : Ventoy fonctionne en général, Apple Silicon non.
- Le Linux live ne voit pas les disques chiffrés BitLocker (le SMART et le scan de surface restent possibles, pas la lecture des fichiers).
