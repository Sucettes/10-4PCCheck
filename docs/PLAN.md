# 10-4 PCCheck · Plan de projet

Document de référence pour reprendre le projet à tout moment. Il contient les décisions prises, l'architecture visée, ce qui reste à faire et ce qui doit encore être validé.

Convention utilisée dans ce document :
- **Fait** : vérifié dans une documentation ou une source.
- **Hypothèse** : probable, mais à valider par un prototype ou un test.
- **Décision** : choix arrêté avec le propriétaire du projet.

Dernière mise à jour : 2026-09-29 (fin de journée).

---

## 1. But

Outil personnel de diagnostic, sur une clé USB, sans installation sur la machine analysée.

Cas d'utilisation :
1. **Achat d'un ordinateur usagé.** Je branche la clé chez le vendeur, je lance l'analyse complète, j'obtiens un rapport (HTML + PDF) avec un verdict vert, jaune ou rouge et les données brutes.
2. **Achat d'un disque usagé.** Je branche le SSD ou le disque dur sur mon ordinateur et j'analyse ce disque seulement.
3. **Achat d'un téléphone Android usagé.** Je branche le téléphone à l'ordinateur et l'outil l'analyse par ADB.
4. **Récupération de fichiers supprimés** avec PhotoRec, derrière une interface graphique.

Usage personnel uniquement, pas de distribution.

---

## 2. Décisions

| Sujet | Décision |
|---|---|
| Plateformes | Windows 10 et 11. Linux (distros à confirmer, défaut : Ubuntu LTS, Fedora, Linux Mint). |
| Mode d'exécution | Les deux : app lancée dans l'OS de la machine (priorité), puis clé bootable en complément. |
| Stack | Tauri 2, moteur en Rust, interface web. React + TypeScript par défaut (pas de préférence exprimée). |
| Disques | `smartctl` (smartmontools 7.5 ou plus) embarqué, sortie JSON. |
| Téléphone | Analyse d'un Android usagé par ADB depuis le PC. Pas d'app Android. |
| Récupération | PhotoRec (CGSecurity) embarqué, appelé en sous-processus. Aucune restriction sur les disques sources : l'utilisateur obtient lui-même les permissions. |
| Tests longs ou qui écrivent | Inclus, derrière une confirmation explicite. |
| Rapport | HTML et PDF. Verdict vert / jaune / rouge + données brutes complètes. |
| Langue | Français (hypothèse, pas confirmé). |
| Distribution | Personnelle. Pas de signature de code prévue : l'avertissement SmartScreen sera contourné à la main. |
| Priorité | Mode « un seul disque » d'abord, puis analyse complète. |
| UI | Mode clair uniquement. Cartes et barre latérale, version 1 de la maquette passée en clair (voir section 6). |

---

## 3. Contraintes connues

### Droits administrateur (fait)
- Windows : lire le SMART exige l'élévation UAC. L'exécutable doit porter un manifeste `requireAdministrator`.
- Linux : `sudo` ou `pkexec` requis pour `smartctl`, `dmidecode` et la lecture brute des disques.
- Chez un vendeur, il faut qu'il accepte l'invite admin ou fournisse le mot de passe.

### Lancement depuis la clé (fait)
- Windows bloque l'autorun depuis une clé USB depuis Windows 7. On lance l'exécutable à la main.
- Exécutable non signé : SmartScreen affichera « Windows a protégé votre ordinateur ». Il faut cliquer sur « Informations complémentaires », puis « Exécuter quand même ».

### Tauri en mode portable
- Windows : Tauri dépend de WebView2. Il est présent sur Windows 11 et sur la plupart des Windows 10 à jour. Tauri permet d'embarquer un runtime WebView2 en version fixe (`webviewInstallMode: fixedRuntime`) pour les machines qui ne l'ont pas. **Hypothèse** : le mode fixe fonctionne depuis une clé sans rien installer, à valider au prototype.
- Linux : Tauri dépend de WebKitGTK. Le bundle AppImage l'embarque. **Hypothèse** : l'AppImage démarre sur Ubuntu, Fedora et Mint sans dépendance manquante (glibc, FUSE). C'est le risque principal de la stack. Si ça échoue, le plan B est Electron.

### Disques via USB (fait)
Le SMART d'un disque dans un boîtier USB ne passe que si le pont USB supporte le passthrough (SAT pour SATA, passthrough propre au fabricant pour NVMe). Source : https://www.smartmontools.org/wiki/USB
- ASMedia ASM1153E : SAT supporté (`-d sat`).
- Realtek RTL9210 / RTL9210B : NVMe supporté (`-d sntrealtek`), variantes RTL9210B reconnues depuis smartmontools 7.5.
- Un ticket signale un boîtier RTL9210B (Sabrent) sans SMART : https://smartmontools.org/ticket/1479. Il faut tester chaque boîtier.

Boîtiers retenus (prix et chipset exact **non vérifiés** sur Amazon.ca, accès bloqué pendant la recherche) :
- SSD SATA 2,5" : UGREEN 2.5" enclosure, ASM1153E, https://www.amazon.ca/dp/B06XWRRMYX
- Alternative câble : adaptateur USB 3.0 vers SATA ASM1153e, https://www.amazon.ca/dp/B0BFX3YF2Z
- M.2 NVMe et SATA : UGREEN M.2, RTL9210B (selon la fiche du même modèle ailleurs), https://www.amazon.ca/dp/B09C8DPNZJ

### Récupération sur SSD (fait)
Sur un SSD avec TRIM actif, les blocs supprimés sont généralement effacés rapidement. La récupération y est quasi nulle. PhotoRec fonctionne bien sur disque dur, clé USB et carte SD. L'interface doit l'indiquer à côté de la source choisie.

### Test de la RAM (fait)
Un test complet exige de démarrer hors de l'OS (MemTest86+). Depuis l'OS, on ne teste que la mémoire libre, donc le résultat est partiel et doit être présenté comme tel.

---

## 4. Architecture

### Découpage
```
crates/
  core/        Disques : smartctl, attributs, cohérence, auto-tests, scan de surface,
               capacité réelle, lanceur de processus partagé. Aucune dépendance à l'UI.
  inventory/   Inventaire matériel (WMI / sysfs), batterie, sécurité Windows,
               test de charge CPU, test RAM partiel.
  android/     Téléphone Android par ADB : collecte, évaluation, vérifications manuelles.
  recovery/    PhotoRec : configuration, destination sur un autre disque, progression.
  report/      Rapport versionné (schema_version), seuils, verdict, HTML, PDF (Typst).
  assemble/    Mesures → rapport (disque, téléphone, machine). Fonctions pures testées.
app/
  src-tauri/   Coquille Tauri 2. Expose les commandes du moteur à l'UI.
  src/         Interface React + TypeScript.
tools/         Binaires tiers embarqués (smartctl, photorec, adb) par plateforme,
               avec un fichier de versions et de sommes SHA-256.
docs/          Ce plan et la documentation.
```

Raison du moteur séparé : il est testable sans UI, et le même JSON alimente l'écran, le rapport HTML et le PDF.

### Disposition de la clé USB
```
/10-4PCCheck/
  windows/     10-4-pccheck.exe, runtime WebView2 fixe, smartctl.exe, photorec_win.exe, adb.exe
  linux/       10-4-pccheck.AppImage, smartctl, photorec_static, adb
  rapports/    Rapports générés (JSON + HTML + PDF)
  recup/       Destination par défaut de PhotoRec
```
Phase bootable : Ventoy sur la clé. La partition de données (exFAT) garde l'outil ci-dessus **et** les ISO (MemTest86+, Linux live). **Fait** : Ventoy démarre des ISO copiés sur sa partition de données. **Fait** : avec Secure Boot actif, Ventoy demande d'enrôler une clé au premier démarrage sur chaque machine, ce qui ajoute une étape chez le vendeur.

### Rapport
- Source unique : le JSON du moteur.
- HTML : fichier autonome (CSS et données en ligne), avec tri et recherche dans les tableaux.
- PDF : **décision (phase 0) : Typst embarqué en Rust** (crates `typst`, `typst-pdf`, `typst-as-lib`). Prototype mesuré : rapport lettre de 2 pages en 38 ms, polices intégrées au PDF, tableaux paginés avec en-têtes répétés, rendu identique sur Windows et Linux. Coût : binaire autonome de 50 Mo et 7 min de compilation à froid, sans importance sur une clé. L'impression de la webview a été écartée : WebView2 et WebKitGTK ont des API différentes, donc deux chemins de code.
- Pied de page : version de l'outil, versions des outils tiers, SHA-256 du JSON pour détecter une modification.

### Règles du verdict
Seuils dans une table de configuration du moteur, pas codés en dur dans l'UI. Valeurs proposées, **à valider** :

| Mesure | Vert | Jaune | Rouge |
|---|---|---|---|
| Santé SSD (usure) | ≥ 90 % | 70 à 89 % | < 70 % |
| Secteurs réalloués | 0 | 1 à 50 | > 50 ou en hausse |
| Secteurs en attente / non corrigibles | 0 | | > 0 |
| Température disque au repos | < 50 °C | 50 à 60 °C | > 60 °C |
| Batterie (capacité / origine) | ≥ 80 % | 60 à 79 % | < 60 % |
| CPU sous charge | pas de bridage | bridage bref | bridage soutenu |
| Gestion d'entreprise (Intune / Autopilot) | aucune | | présente |
| Patch de sécurité Android | < 3 mois | 3 à 12 mois | > 12 mois |
| Compte Google connecté | non | oui (à retirer devant l'acheteur) | |

Verdict global : rouge si un rouge, jaune si au moins un jaune, sinon vert.

---

## 5. Sources de données par module

### Disques
- Inventaire : `smartctl --scan-open -j`.
- Détails : `smartctl -a -j <device>` (ajouter `-d sat` ou `-d sntrealtek` selon le pont USB).
- Auto-tests : `smartctl -t short|long`, suivi par `smartctl -l selftest -j`.
- Scan de surface : lecture brute séquentielle en Rust, en lecture seule. On mesure les secteurs illisibles et les zones lentes.
- Capacité réelle (anti-contrefaçon, comme f3 et H2testw) : écriture de blocs signés puis relecture. Deux modes : espace libre seulement (non destructif pour les fichiers) et disque entier (destructif, confirmation obligatoire).
- Cohérence : écritures totales vs heures, heures vs démarrages, usure vs écritures. Le but est de repérer des compteurs remis à zéro.
- Score de santé : attribut propre au fabricant pour les SSD SATA (ex. B1 Wear Leveling Count chez Samsung), champ `percentage_used` pour NVMe.

### Inventaire Windows (hypothèses de mise en œuvre, à valider)
- WMI : `Win32_Processor`, `Win32_PhysicalMemory`, `Win32_BaseBoard`, `Win32_BIOS`, `Win32_VideoController`, `Win32_NetworkAdapter`.
- Batterie : `root\wmi` `BatteryStaticData.DesignedCapacity`, `BatteryFullChargedCapacity`, `BatteryCycleCount`. Contrôle croisé avec `powercfg /batteryreport`.
- Licence : `SoftwareLicensingProduct` (LicenseStatus).
- BitLocker : `Win32_EncryptableVolume`.
- Azure AD / Intune : `dsregcmd /status`. Autopilot : emplacement dans le registre à confirmer.
- Températures CPU : un pilote noyau est requis. LibreHardwareMonitor utilise PawnIO dans ses versions récentes. L'ancien pilote WinRing0 est signalé par Defender. **À prototyper** : appel depuis Rust (sidecar .NET ou autre voie).

### Inventaire Linux
- `/sys/class/dmi/id/*`, `/proc/cpuinfo`, `/proc/meminfo`, `dmidecode -t memory` (root), `lspci`.
- Batterie : `/sys/class/power_supply/BAT*/` (`energy_full`, `energy_full_design`, `cycle_count`).
- Températures : `/sys/class/hwmon/*`.

### Charge CPU
Test de charge multi-thread écrit en Rust, 5 minutes par défaut. On mesure la température, la fréquence et le bridage thermique.

### Tests interactifs (dans l'UI)
- Clavier : carte des touches qui s'allument à l'appui. Limite : certaines touches (Fn, touches multimédia) ne remontent pas d'événement.
- Écran : couleurs pleines en plein écran pour repérer les pixels morts.
- Webcam et micro : `getUserMedia` dans la webview. **À vérifier** : la gestion des permissions caméra et micro dans Tauri (WebView2 et WebKitGTK).
- Haut-parleurs : son gauche puis droit (Web Audio).
- Ports USB : on débranche et rebranche un périphérique, l'outil détecte l'événement.

### Android (ADB)
- `adb devices -l`. Le débogage USB doit être activé et la clé RSA acceptée sur le téléphone.
- `getprop` : `ro.product.model`, `ro.build.version.release`, `ro.build.version.security_patch`, `ro.boot.verifiedbootstate`, `ro.boot.flash.locked`, `ro.csc.sales_code` (Samsung, nom de propriété à vérifier).
- `dumpsys battery` : niveau, température, santé. Le nombre de cycles dépend de la version d'Android et du fabricant.
- `dumpsys account` : comptes connectés (**hypothèse** : accessible depuis le shell ADB sans root).
- `dpm list-owners` : gestion d'entreprise (Device Owner).
- `df /data` : stockage. L'usure de la puce (UFS/eMMC) exige le root, on l'affiche comme non disponible.
- L'IMEI n'est pas lisible sans privilèges. L'UI renvoie à `*#06#` et à une vérification en ligne.
- Licence de redistribution d'ADB (platform-tools) : à vérifier, peu critique pour un usage personnel.

### Récupération (PhotoRec)
- Binaires officiels CGSecurity : Windows (`photorec_win.exe`, dans l'archive TestDisk win64) et Linux statique.
- Mode non interactif : `photorec /log /d <destination> /cmd <device> <options>,search`.
- Progression : **à valider**. Deux pistes : analyser la sortie ou le `photorec.log`, ou compter les fichiers écrits dans `recup_dir.*`.
- Règle stricte dans l'UI : la destination doit être sur un autre disque que la source.

---

## 6. Direction de l'interface

Maquette (6 écrans, privée) : https://claude.ai/artifact/51wHFuPwGt3ErzNnPr32LY

Historique des choix :
1. Version 1 : cartes arrondies, barre latérale, typographie Space Grotesk + IBM Plex, en mode sombre.
2. Version 2 : style application de bureau dense (arborescence, onglets, tableaux serrés, façon pgAdmin). **Rejetée** : moins bonne que la version 1.
3. **Version retenue** : la version 1, passée en mode clair.

Règles :
- **Mode clair uniquement**, pas de mode sombre.
- Barre latérale à gauche (Accueil, Analyse complète, Disques, Téléphone, Récupération, Rapports) avec l'état admin en bas.
- Contenu en cartes blanches sur fond gris très pâle, coins de 10 à 14 px, bordures fines.
- Un bandeau de verdict en haut des écrans de résultat (icône, titre, résumé, compteurs OK / avertissements / critiques).
- Typographie : Space Grotesk pour les titres et les chiffres clés, IBM Plex Sans pour le texte, IBM Plex Mono pour les valeurs brutes. Les polices seront embarquées dans l'app (pas de Google Fonts en ligne chez le vendeur).
- Chaque état porte un libellé texte en plus de sa couleur (Bon, Usée, À négocier, Critique).
- Interface dense (demande du propriétaire, 2026-09-29) : texte de base 13 px, boutons et cibles cliquables de 30 à 32 px, marges réduites, grilles qui ajoutent des colonnes sur un écran large. C'est un outil de bureau utilisé à la souris : les 44 px des interfaces tactiles ne s'imposent pas.

Palette :

| Rôle | Couleur |
|---|---|
| Fond de l'application | `#f6f7f9` |
| Cartes, barre latérale | `#ffffff` |
| Bordures | `#e3e6eb` (normale), `#cfd5de` (boutons secondaires) |
| Texte / texte secondaire | `#1a1d23` / `#5d6574` |
| Accent (bouton principal, liens) | `#2459d6` |
| Élément actif (menu, sélection) | fond `#e8effc`, texte `#1f4fb8` |
| Vert (fond / texte / point) | `#e3f5ea` / `#177a47` / `#1f9d5b` |
| Jaune (fond / texte / bordure) | `#fff3d6` / `#8a5a00` / `#f0d48a` |
| Rouge (fond / texte / bordure) | `#fdeceb` / `#b42318` / `#f0b8b4` |

Écrans de la maquette : Accueil, Analyse complète, Un disque (inspiré de CrystalDiskInfo), Téléphone Android, Récupération, Rapport PDF.

### Vérification de l'interface
Claude vérifie lui-même le rendu de toute modification d'interface avant de la livrer. Ce n'est pas au propriétaire de le faire. Méthode (Playwright + Chromium) :
1. Rendre chaque écran à sa taille réelle, avec les vraies polices chargées. En cas d'échec du chargement d'une police, le contrôle échoue.
2. Contrôles automatiques : contenu qui dépasse le cadre, texte tronqué, contraste des textes (4,5:1, ou 3:1 pour le gros texte).
3. Capture d'écran de chaque écran, relue pour les défauts qu'un script ne voit pas : libellés qui se replient, éléments collés au bord, espaces vides excessifs.

Première passe sur la maquette (2026-09-29) : 2 débordements corrigés (Accueil, Un disque), 3 libellés raccourcis (Analyse complète), hauteurs ajustées. Aucun problème de contraste.

---

## 7. Feuille de route

### Phase 0 · Prototype de validation
Objectif : lever les risques de la stack avant d'écrire les fonctionnalités.
- [x] Squelette Tauri 2 + React + TypeScript qui affiche une fenêtre.
- [x] Build Windows portable (exe + `tools/smartctl.exe`), lancé et vérifié en CI (Windows Server). Reste à confirmer sur Windows 10 et 11 depuis une clé : test final.
- [ ] Runtime WebView2 « version fixe » dans `webview2/` : le code le prend en charge, non testé (WebView2 est déjà présent sur les runners).
- [x] Manifeste `requireAdministrator` présent dans l'exe (vérifié en CI). Invite UAC réelle : test final.
- [x] AppImage Linux : autotest et capture réussis sur Ubuntu 22.04, Fedora 44 et Linux Mint 22 (conteneurs, voir « Résultats »).
- [ ] Élévation Linux via `pkexec` : non faite. L'app détecte les droits et affiche « Droits limités » ; le LISEZMOI de la clé dit de lancer l'AppImage avec `sudo`. Voir « Limites connues ».
- [x] Appel de `smartctl -j` embarqué depuis Rust et affichage dans l'interface.
- [x] Choix de la génération PDF : Typst embarqué (section 4).

Critère de sortie : les quatre points de lancement passent. Sinon, décision Electron.
**État : Tauri validé sur Linux et en CI Windows. Pas de bascule vers Electron.** Reste le test final sur le PC du propriétaire.

#### Résultats (2026-09-29)
- Moteur : 17 tests (analyse JSON SATA/NVMe, disque défaillant, pont USB sans SMART, délai maximal, appels concurrents), dont 2 sur des sorties réelles de smartctl.
- smartctl 7.5 compilé en statique (`tools/build-smartctl-linux.sh`), embarqué dans l'AppImage.
- AppImage de 82 Mo construite sur Ubuntu 22.04 (`tools/build-appimage.sh`). Testée par `tools/dev/test-appimage-distros.sh`.
  - Constat : sur une image de conteneur minimale, il manque fontconfig, freetype, X11, xcb, Wayland, fribidi, harfbuzz et Mesa. C'est normal : la liste d'exclusion AppImage suppose ces bibliothèques présentes sur tout bureau Linux. Les tests utilisent donc des images de bureau (Ubuntu 22.04 + GTK3 et Mesa, Fedora 44 XFCE, Mint 22).
- CI Windows : build, manifeste vérifié, autotest réussi en mode élevé, lecture réelle des disques virtuels Azure. Capture d'écran Windows relue (copiée en base64 dans le journal CI).
- Interface vérifiée par captures d'écran sur les 3 distros et avec le simulateur `tools/dev/fake-smartctl` (4 cas de disques).

#### Suites identifiées
- Élévation Linux : lancer l'interface en utilisateur et un assistant privilégié unique via `pkexec` (une seule demande de mot de passe). Lancer toute l'interface en root pose problème sous Wayland.
- Disques virtuels et certains SCSI : smartctl renvoie 0 °C quand la température n'existe pas. Corrigé : 0 °C est traité comme « inconnue ».
- Écrans de 1024 x 768 et 1366 x 768 : la fenêtre (1280 x 720) se réduit à l'écran et se centre au démarrage, le contenu défile. Vérifié par captures à 1024 x 768, 1366 x 768 et 1920 x 1080.
- Quota de stockage d'artefacts GitHub du compte atteint le 2026-09-29 : les téléchargements CI sont non bloquants. Pour récupérer les fichiers de la clé, libérer le quota (supprimer d'anciens artefacts) ou publier via une release GitHub.
- Réseau de la session cloud de Claude : l'étape finale de linuxdeploy échoue derrière le proxy ; `tools/build-appimage.sh` accepte un runtime AppImage fourni à la main (voir l'en-tête du script). Aucun impact en CI.

#### Test final sur le PC du propriétaire (environ 10 minutes)
1. Récupérer les fichiers produits par la CI (artefacts `10-4-pccheck-windows` et `10-4-pccheck-linux`) et les copier sur la clé :
   `windows/10-4-pccheck.exe`, `windows/tools/smartctl.exe`, `linux/10-4-pccheck.AppImage`.
2. Brancher la clé sur le PC Windows et lancer `windows/10-4-pccheck.exe`.
3. SmartScreen : « Informations complémentaires », puis « Exécuter quand même ». Noter si Defender bloque.
4. Invite UAC : accepter. Vérifier « Mode administrateur » en bas à gauche.
5. Vérifier que le Samsung 860 EVO apparaît avec température, heures (environ 3 864 h) et état « Bon ».
6. Si un boîtier USB est disponible, y brancher un disque et vérifier s'il est lu ou marqué « Illisible ».
7. Envoyer à Claude une capture d'écran et, si possible, la sortie de `windows\tools\smartctl.exe -a -j /dev/sda` (invite de commandes en administrateur) : elle deviendra un vrai jeu de test, numéro de série masqué.

#### Qui vérifie quoi
Claude vérifie tout ce qui peut l'être sans matériel réel. Le propriétaire ne fait qu'un test final sur sa machine.

| Point | Où | Ce qui est vérifié | Limite |
|---|---|---|---|
| AppImage Linux | Docker dans la session cloud de Claude, conteneurs Ubuntu 22.04, Fedora 41, Linux Mint 22 + Xvfb | Démarrage sans dépendance manquante, rendu de la fenêtre, capture d'écran relue | Pas de vrai GPU ni de Wayland. Lancement avec `--appimage-extract-and-run` (pas de FUSE en conteneur) |
| Build et lancement Windows | GitHub Actions, runner `windows-latest` | Compilation, présence du manifeste admin dans l'exe, démarrage, contenu rendu par la webview (écrit dans le journal par un mode d'autotest) | Windows Server, pas Windows 10/11. UAC désactivé sur les runners, donc l'invite admin n'est pas testable. WebView2 déjà installé |
| smartctl depuis Rust | Docker et Actions | Appel du binaire embarqué, analyse du JSON sur des sorties enregistrées (SATA, NVMe, pont USB sans SMART, disque défaillant) | Pas de vrai disque avec SMART dans le cloud |
| Génération PDF | Docker | Rendu du PDF, capture relue | Aucune |
| **Test final** | **Machine du propriétaire** | Clé USB sur son PC Windows : SmartScreen, invite UAC, lecture SMART réelle du Samsung 860 EVO | Environ 10 minutes, liste de contrôle fournie par Claude |

État constaté le 2026-09-29 : Docker fonctionne dans la session (démarrer `dockerd` au besoin). Fedora 41 et Mint 22 téléchargés. Ubuntu bloqué temporairement par les quotas anonymes de Docker Hub (429) et d'ECR ; plan B : image construite avec `debootstrap` depuis archive.ubuntu.com (accessible). Pas de `/dev/kvm`, donc pas de machine virtuelle Windows locale. Le dépôt est privé : les minutes GitHub Actions comptent dans le quota du compte (Windows compte double, quota à vérifier sur le compte).

### Phase 1 · Un seul disque (V1)
- [x] Détection des disques (internes et USB) avec le type de pont. Doublons Intel RST (`/dev/csmiN,P`) retirés par modèle + numéro de série.
- [x] Écran « Un disque » selon la maquette : onglets par disque, vie restante, température, données écrites, fiche technique, attributs SMART (décimal / hexa), journal de santé NVMe. Info-bulles explicatives sur chaque donnée. Reste : boutons des tests (ci-dessous).
- [x] Traduction française des attributs SMART courants (par nom smartctl, pas par ID : les ID 170+ changent de sens selon le fabricant). État par attribut : OK, à surveiller, échec.
- [x] Score de santé par fabricant (SATA : 177, 202, 231, 233, SSD seulement) et `percentage_used` (NVMe).
- [x] Vérifications de cohérence (module `checks` du moteur) : compteurs d'erreurs, écritures vs heures, heures vs démarrages (sessions très courtes ou très longues), usure vs écritures complètes, coupures brutales NVMe. Seuils en constantes, à ajuster sur de vrais disques.
- [x] Auto-tests SMART court et long avec suivi (module `selftest` : `-t short|long`, `-X`, état par `-c -l selftest`), progression, annulation, 5 derniers résultats. ATA et NVMe. Formats reconstruits : à confirmer sur un vrai test (NVMe sous Windows notamment).
- [x] Scan de surface en lecture seule (sans cache, zones illisibles localisées à 64 Kio, blocs lents, profil de débit).
- [x] Test de capacité réelle sur l'espace libre (méthode H2testw/f3, distingue secteurs abîmés et écrasés). Le mode « disque entier » destructif n'est pas fait : voir « Limites connues ».
- [x] Tests unitaires du moteur sur des sorties `smartctl` enregistrées (SATA, NVMe, pont USB sans SMART, disque défaillant, auto-tests). Sorties reconstruites : aucune sortie réelle d'un disque du propriétaire dans le dépôt.

### Phase 2 · Rapport
- [x] Schéma JSON versionné du rapport (`crates/report`, `SCHEMA_VERSION = 1`).
- [x] Moteur de verdict avec la table de seuils (`Thresholds`, valeurs de la section 4).
- [x] Export HTML autonome (tri, recherche, données brutes, empreinte SHA-256 du JSON).
- [x] Export PDF (Typst 0.15 embarqué, polices DejaVu incluses, données passées en JSON : aucune injection possible).
- [x] Liste des rapports dans l'écran Rapports et sur l'accueil.

### Phase 3 · Analyse complète
- [x] Inventaire matériel Windows (WMI) et Linux (sysfs, /proc, dmidecode, lspci).
- [x] Batterie (capacité d'origine et actuelle, cycles, santé).
- [x] Licence Windows, BitLocker, Secure Boot, TPM, Azure AD / domaine (dsregcmd), Intune, Autopilot.
- [ ] Températures CPU sous Windows : zones ACPI seulement (souvent absentes). Pilote noyau (PawnIO / LibreHardwareMonitor) non intégré. Linux : hwmon.
- [x] Test de charge CPU : bridage détecté par la baisse de débit (sans pilote), référence prise après la fenêtre de turbo (~30 s), erreurs de calcul détectées.
- [x] Test RAM partiel en OS (50 % de la mémoire disponible, 5 motifs).
- [x] Tests interactifs : clavier (codes physiques), pixels morts, webcam, micro, haut-parleurs G/D, pavé tactile. Ports USB : dans la liste « devant le vendeur » (branche la clé dans chaque port).

### Phase 4 · Téléphone Android
- [x] ADB recherché dans tools/ de la clé (`tools/fetch-tools-windows.ps1` le télécharge), détection et guide pour activer le débogage USB.
- [x] Collecte (voir section 5) et écran selon la maquette. Aucune adresse de compte ni série en clair dans le rapport.
- [x] Liste de vérifications manuelles enregistrée dans le rapport.

### Phase 5 · Récupération
- [x] PhotoRec lancé en mode `/cmd` (console sans fenêtre sous Windows, PDCurses exige une vraie console). Binaire à placer dans tools/testdisk/ (`tools/fetch-tools-windows.ps1`).
- [x] Écran de paramètres (source, types, destination obligatoirement sur un autre disque), progression et fichiers trouvés.
- [x] Avertissement TRIM quand la source est un SSD.
- [x] Deuxième moteur : The Sleuth Kit (`fls` + `tsk_recover`), récupération par le système de fichiers avec **noms et dossiers conservés** (NTFS, FAT, exFAT, ext). Validé sur une image FAT16 générée.
- [x] TestDisk dans un terminal intégré (pseudo-terminal ConPTY via `portable-pty`, affichage xterm.js), pour les partitions perdues et la restauration manuelle ; fenêtre de console séparée en secours. Validé : TestDisk 7.2 s'affiche et répond au clavier dans le pseudo-terminal.

### Phase 6 · Clé bootable
- [x] Procédure Ventoy + outil sur la partition de données : `docs/CLE-BOOTABLE.md` ; `tools/assemble-usb.ps1` prépare le dossier. L'installation de Ventoy efface la clé : faite à la main.
- [x] ISO MemTest86+ (procédure).
- [x] Linux live : Ubuntu LTS proposé par défaut (procédure).
- [x] Procédure d'enrôlement Secure Boot documentée.

---

### Limites connues (fin de la première version, 2026-09-29)
- **Test de capacité « disque entier » (destructif) non fait** : il faudrait verrouiller et démonter les volumes du disque sous Windows, et une erreur de disque cible effacerait des données. Le mode espace libre couvre le cas d'achat (clé ou carte vide, formatée).
- **Températures CPU sous Windows** : pas de pilote noyau ; seules les zones ACPI sont lues quand elles existent.
- **Élévation Linux** : pas d'assistant `pkexec` ; lancer l'AppImage avec `sudo`.
- **Formats non vérifiés sur du vrai matériel** : auto-tests SMART (surtout NVMe sous Windows), sorties adb (`dpm`, champs Samsung), PhotoRec réel, batterie de portable sous Windows. À confirmer au premier usage réel ; les analyseurs sont isolés et testés sur des sorties reconstruites.
- **Outils tiers** : adb et PhotoRec ne sont pas dans le dépôt ; `tools/fetch-tools-windows.ps1` les télécharge depuis leurs sources officielles avec leurs sommes SHA-256. Pour Linux, placer `adb` et `photorec_static` dans `linux/tools/` de la clé.
- **Runtime WebView2 fixe** : pris en charge par le code, jamais testé sur une machine sans WebView2.

---

## 8. Questions ouvertes

1. React + TypeScript pour l'interface : à confirmer.
2. Distributions Linux ciblées : à confirmer (défaut : Ubuntu LTS, Fedora, Linux Mint).
3. Langue du rapport : français seulement, ou aussi anglais ?
4. Seuils du verdict (section 4) : à valider.
5. Test de charge GPU : utile ou non ?
6. Nom final de l'outil et de l'exécutable.
7. Distribution Linux du live USB (phase 6).

---

## 9. Journal

| Date | Travail |
|---|---|
| 2026-09-29 | Analyse de faisabilité, décisions (section 2), maquette UI (version 1 sombre, version 2 style bureau rejetée, version 1 passée en clair retenue), choix des boîtiers USB, création de ce plan. |
| 2026-09-29 | Phase 0 : moteur Rust + smartctl, app Tauri + React, smartctl statique, AppImage testée sur 3 distros, CI Windows et Linux, décision PDF (Typst). Reste le test final sur le PC du propriétaire. |
| 2026-09-29 | Test final phase 0 sur le PC du propriétaire (Windows 11, exe compilé en local : invite UAC et lecture SMART de 4 NVMe OK ; SmartScreen non testable, exe non téléchargé). Test chez un ami : doublon Intel RST corrigé. Phase 1 : vie restante, info-bulles, écran « Un disque ». CI : artefacts gardés 1 jour (quota de 0,5 Go atteint). |
| 2026-09-29 | Phases 1 à 6 : cohérence, auto-tests, scan de surface, capacité réelle ; crates inventory, android, report, recovery, assemble ; écrans Accueil, Analyse complète, Téléphone, Récupération, Rapports ; rapports JSON/HTML/PDF ; scripts de la clé et procédure bootable. 229 tests du moteur. |
