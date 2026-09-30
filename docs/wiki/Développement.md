# Développement

Tauri 2 (Rust) et React/TypeScript. Moteur en crates Rust indépendantes de l'interface, testables sans droits administrateur.

| Dossier | Rôle |
|---|---|
| `crates/core` | smartctl, SMART, lectures sans cache, scan de surface, capacité, vitesse, âge, liaison USB |
| `crates/inventory` | Inventaire matériel (WMI, Linux), tests de charge CPU, RAM, GPU |
| `crates/android` | adb, collecte et évaluation d'un téléphone |
| `crates/recovery` | PhotoRec, The Sleuth Kit, TestDisk dans un pseudo-terminal |
| `crates/report` | Modèle du rapport, verdict, HTML, PDF (Typst embarqué) |
| `crates/assemble` | Construction des rapports à partir des mesures |
| `app/` | Application Tauri (`src-tauri/`) et interface React (`src/`) |
| `tools/` | Téléchargement des outils tiers, assemblage de la clé, scripts de CI et de développement |

## Construire (Windows)

Prérequis : Rust 1.92 ou plus (toolchain MSVC), Build Tools C++ de Visual Studio, Node 22.

```
./tools/fetch-tools-windows.ps1     # outils tiers, une fois
./tools/assemble-usb.ps1 -Build     # exe + dossier de la clé dans dist-usb/
```

## Tests

```
cargo test --workspace --exclude pccheck-app
cargo clippy --workspace --all-targets
cd app && npx tsc --noEmit
```

Tests Linux, dont les tests réels avec PhotoRec, TestDisk et The Sleuth Kit des paquets Debian : `tools/dev/docker-linux-tests.sh` (voir son en-tête).

L'application a un mode d'autotest : `PCCheck.exe --self-test sortie.json` affiche l'interface, écrit ce qu'elle a rendu, puis quitte.

## Branches, CI et releases

- Une branche par fonctionnalité, fusionnée dans `master` par pull request ; une fonctionnalité importante est découpée en **pull requests empilées** (moteur, puis application et rapport, puis interface), fusionnées du bas vers le haut. Détails dans `CLAUDE.md`.
- Les issues étiquetées `agentflySucettes` sont traitées par AgentFly (`agentfly.yml`).
- La CI (`.github/workflows/ci.yml`) vérifie le format, clippy et les tests, puis construit l'AppImage Linux et l'exécutable Windows (avec autotest).
- Chaque mise à jour de `master` qui passe publie une release (`tools/ci/release.sh`).
- Le wiki est publié depuis `docs/wiki/` à chaque mise à jour de `master` : modifie les pages dans le dépôt, jamais directement sur GitHub. Une pull request qui change un comportement met à jour sa documentation (règle détaillée dans `CLAUDE.md`).

## Données personnelles

Le dépôt est public. Aucune sortie réelle d'un disque, d'un téléphone ou d'une machine ne doit y entrer : les tests utilisent des sorties reconstruites, avec des numéros de série et des comptes inventés.
