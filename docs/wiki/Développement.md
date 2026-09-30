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

- Chaque fonctionnalité a sa branche, créée depuis `dev`, et revient dans `dev` par pull request (squash). Ces PR ne lancent **aucune CI**, pour garder le flux rapide ; une fonctionnalité importante est découpée en **pull requests empilées**. Détails dans `CLAUDE.md`.
- Quand `dev` est prête, une PR `dev` → `master` (commit de fusion) lance `verification.yml` : format, clippy et tests sous Linux, clippy et tests sous Windows.
- Chaque arrivée sur `master` lance `release.yml` : AppImage Linux et paquet Windows compilés au maximum d'optimisation, en parallèle, vérifiés par l'autotest, puis publiés en release (`tools/ci/release.sh`, une release par commit), et wiki mis à jour.
- Chaque PR porte ses étiquettes : un type (`bogue`, `amélioration`, `documentation`, `ci`, `dépendances`) et les domaines touchés (`disques`, `machine`, `téléphone`, `récupération`). Détails dans `CLAUDE.md`.
- Les issues étiquetées `agentflySucettes` sont traitées par AgentFly (`agentfly.yml`) ; AgentFly et Dependabot ouvrent leurs PR vers `dev`.

## Données personnelles

Le dépôt est public. Aucune sortie réelle d'un disque, d'un téléphone ou d'une machine ne doit y entrer : les tests utilisent des sorties reconstruites, avec des numéros de série et des comptes inventés.
