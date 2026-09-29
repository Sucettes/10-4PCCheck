# 10-4 PCCheck · Plan de projet

Document de référence pour reprendre le projet à tout moment. Il contient les décisions prises, l'architecture visée, ce qui reste à faire et ce qui doit encore être validé.

Convention utilisée dans ce document :
- **Fait** : vérifié dans une documentation ou une source.
- **Hypothèse** : probable, mais à valider par un prototype ou un test.
- **Décision** : choix arrêté avec le propriétaire du projet.

Dernière mise à jour : 2026-09-29.

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
| UI | Mode clair uniquement. Style application de bureau (voir section 6). |

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
  core/        Moteur de collecte en Rust. Aucune dépendance à l'UI.
               Produit un rapport JSON versionné (champ schema_version).
  cli/         Binaire en ligne de commande sur le moteur (debug, tests, scripts).
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
- PDF : **à décider au prototype**. Option recommandée : générer le PDF en Rust avec Typst embarqué (rendu identique sur Windows et Linux). Autre option : l'impression PDF de la webview, mais WebView2 et WebKitGTK n'ont pas la même API, ce qui donne deux chemins de code.
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

Retour du propriétaire : la première version (sombre, grandes cartes arrondies) n'était pas laide mais faisait « IA générique ». La version retenue suit ces règles :
- **Mode clair uniquement**, pas de mode sombre.
- Style application de bureau. Références : pgAdmin 4, MongoDB Compass, logiciels de gestion.
- Barre de menu en haut (Fichier, Analyse, Outils, Aide), arborescence des appareils à gauche, onglets, barre d'état en bas.
- Tableaux denses (lignes de 25 à 32 px), panneaux avec barre de titre grise, coins de 2 à 3 px.
- Police du système : Segoe UI sous Windows, Ubuntu ou Noto Sans sous Linux. Consolas ou DejaVu Sans Mono pour les valeurs brutes.
- Pas de dégradés, pas de grandes cartes, pas d'écran d'accueil façon page marketing.
- Chaque état porte un libellé texte en plus de sa couleur (VERT/BON, JAUNE/À VÉRIFIER, ROUGE/CRITIQUE).

Palette de la maquette :

| Rôle | Couleur |
|---|---|
| Barre de menu | `#2d5a88` |
| Fond de l'application | `#eef0f3` |
| Panneaux | `#ffffff`, titre `#f5f6f8` |
| Bordures | `#c9ced6`, `#d5d9df` |
| Texte / texte secondaire | `#1f2328` / `#5b6472` |
| Lien, bouton principal | `#1f5fa8` |
| Sélection dans l'arbre | `#cfe0f5` |
| Vert (fond / texte) | `#dff3e4` / `#1e6b34` |
| Jaune (fond / texte) | `#fff0cc` / `#7a5200` |
| Rouge (fond / texte) | `#fde2e0` / `#a1241b` |

Écrans de la maquette : Accueil, Analyse complète, Un disque (inspiré de CrystalDiskInfo), Téléphone Android, Récupération, Rapport PDF.

---

## 7. Feuille de route

### Phase 0 · Prototype de validation
Objectif : lever les risques de la stack avant d'écrire les fonctionnalités.
- [ ] Squelette Tauri 2 + React + TypeScript qui affiche une fenêtre.
- [ ] Build Windows portable avec WebView2 fixe, lancé depuis une clé sur Windows 10 et 11 sans installation.
- [ ] Élévation admin au lancement (manifeste) sur Windows 10 et 11.
- [ ] AppImage Linux lancée depuis la clé sur Ubuntu LTS, Fedora et Linux Mint (élévation via `pkexec`).
- [ ] Appel de `smartctl -j` embarqué depuis Rust et affichage brut du JSON.
- [ ] Choix de la génération PDF (Typst embarqué ou impression webview).

Critère de sortie : les quatre points de lancement passent. Sinon, décision Electron.

### Phase 1 · Un seul disque (V1)
- [ ] Détection des disques (internes et USB) avec le type de pont.
- [ ] Écran « Un disque » selon la maquette : santé, température, fiche technique, attributs SMART (décimal / hexa).
- [ ] Traduction française des attributs SMART courants.
- [ ] Score de santé par fabricant (SATA) et `percentage_used` (NVMe).
- [ ] Vérifications de cohérence.
- [ ] Auto-tests SMART court et long avec suivi.
- [ ] Scan de surface en lecture seule.
- [ ] Test de capacité réelle (espace libre, puis disque entier avec confirmation).
- [ ] Tests unitaires du moteur sur des sorties `smartctl` enregistrées (SATA, NVMe, pont USB sans SMART, disque défaillant).

### Phase 2 · Rapport
- [ ] Schéma JSON versionné du rapport.
- [ ] Moteur de verdict avec la table de seuils.
- [ ] Export HTML autonome.
- [ ] Export PDF.
- [ ] Liste des rapports dans l'arborescence.

### Phase 3 · Analyse complète
- [ ] Inventaire matériel Windows et Linux.
- [ ] Batterie (capacité, cycles).
- [ ] Licence Windows, BitLocker, Intune, Autopilot.
- [ ] Températures (prototype PawnIO / LibreHardwareMonitor sous Windows).
- [ ] Test de charge CPU avec courbe de température.
- [ ] Test RAM partiel en OS.
- [ ] Tests interactifs : clavier, écran, webcam, micro, haut-parleurs, ports USB.

### Phase 4 · Téléphone Android
- [ ] ADB embarqué, détection et guide pour activer le débogage USB.
- [ ] Collecte (voir section 5) et écran selon la maquette.
- [ ] Liste de vérifications manuelles enregistrée dans le rapport.

### Phase 5 · Récupération
- [ ] PhotoRec embarqué (Windows et Linux).
- [ ] Écran de paramètres, progression et liste des fichiers trouvés.
- [ ] Avertissement TRIM quand la source est un SSD.

### Phase 6 · Clé bootable
- [ ] Ventoy sur la clé, l'outil portable sur la partition de données.
- [ ] ISO MemTest86+.
- [ ] Linux live avec l'AppImage de l'outil (choix de la distribution à faire).
- [ ] Procédure d'enrôlement Secure Boot documentée.

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
| 2026-09-29 | Analyse de faisabilité, décisions (section 2), maquette UI en 2 versions (sombre rejetée, claire retenue), choix des boîtiers USB, création de ce plan. Aucun code écrit. |
