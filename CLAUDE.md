# CLAUDE.md — PCCheck

Parle-moi toujours en français : messages, questions, descriptions de commandes, commits, pull requests, textes de l'interface et commentaires du code.

PCCheck est un outil de diagnostic portable, sur clé USB, pour vérifier un appareil d'occasion avant l'achat (ordinateur, disque, clé USB, téléphone Android) et récupérer des fichiers supprimés. Chaque analyse donne un verdict (Bon achat, À négocier, À éviter) et un rapport HTML/PDF. Documentation utilisateur : `docs/wiki/` (publiée sur le wiki GitHub). Plan, décisions et limites connues : `docs/PLAN.md`.

## Dépôt public : aucune donnée personnelle

Le dépôt, ses issues et son wiki sont publics. N'y mets jamais : nom, courriel, nom d'utilisateur Windows, nom de machine, chemins personnels (`C:\Users\...`), numéros de série réels, sorties réelles de smartctl, adb ou WMI d'une vraie machine, clés, jetons. Les tests utilisent des sorties **reconstruites** avec des séries et des comptes inventés (`R58N00000XX`, `compte1@example.com`). Vérifie un diff avant chaque commit.

## Documentation toujours à jour

La documentation fait partie de la fonctionnalité : **une pull request qui change un comportement met à jour sa documentation dans la même pull request**, sinon elle n'est pas terminée.

| Ce qui change | À mettre à jour |
|---|---|
| Écran, mesure, seuil, message visible | La page du wiki concernée (`docs/wiki/`), l'info-bulle (`app/src/hints.ts`) |
| Seuil du verdict ou échelle | `docs/wiki/Rapports-et-verdict.md` ou `Disques.md`, et les tests qui le fixent |
| Installation, clé USB, outils tiers | `docs/wiki/Installation.md`, `docs/LISEZMOI-CLE.txt`, `README.md` |
| Commandes, organisation, convention, piège | Ce fichier (`CLAUDE.md`) et `docs/wiki/Développement.md` |
| Décision, limite connue, question tranchée | `docs/PLAN.md` |

Le wiki GitHub est publié automatiquement depuis `docs/wiki/` à chaque mise à jour de `master` (job `wiki` de la CI) : ne le modifie jamais directement sur GitHub, ta modification serait écrasée. Liens entre pages : `[[Nom de la page]]`.

## Organisation

| Dossier | Rôle |
|---|---|
| `crates/core` | smartctl (JSON), attributs SMART, cohérence, fonctionnalités ATA (`identify`), lectures sans cache (`rawio`), scan de surface, capacité réelle, vitesse (`speed`), âge (`age`), liaisons USB (`usb`) et PCIe (`pcie`), arbre des périphériques Windows (`devtree`), processus (`process`) |
| `crates/inventory` | Inventaire (WMI Windows, sysfs Linux), charge CPU, test RAM, capteurs GPU |
| `crates/android` | adb : appareils, collecte, évaluation, liste de vérifications |
| `crates/recovery` | PhotoRec, The Sleuth Kit (fls, tsk_recover, icat), TestDisk en pseudo-terminal, destination vérifiée sur un autre disque |
| `crates/report` | Modèle du rapport, niveaux, seuils (`Thresholds`), verdict, HTML, PDF (Typst embarqué) |
| `crates/assemble` | Construit les rapports à partir des mesures de la session (`Results`) |
| `app/src-tauri` | Application Tauri 2 : commandes, tâches longues (`jobs.rs`), cache de session, terminal |
| `app/src` | Interface React + TypeScript (routage par hash, pas de bibliothèque d'état) |
| `tools/` | Outils tiers, assemblage de la clé, CI (`tools/ci`), GitHub (`tools/github`), développement (`tools/dev`) |

Principe : **cœur fonctionnel, coquille impérative**. La logique (analyse, évaluation, seuils) est dans des fonctions pures des crates, testées sans matériel ni droits ; l'application ne fait que brancher, lancer et afficher.

## Commandes

```
cargo test --workspace --exclude pccheck-app      # moteur (pccheck-app exige les droits admin sous Windows)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cd app && npx tsc --noEmit                        # typage de l'interface
./tools/assemble-usb.ps1 -Build                   # exe + dossier de la clé dans dist-usb/ (Windows)
tools/dev/docker-linux-tests.sh                   # tests Linux et tests réels (PhotoRec, TSK, TestDisk) en conteneur
```

Tests réels ignorés par défaut (`#[ignore]`) : voir l'en-tête de chaque fichier de `crates/recovery/tests/real_*.rs` et `crates/core/src/usb.rs` (`real_disks_links`).

Autotest de l'application : `PCCheck.exe --self-test sortie.json` (avec `__COMPAT_LAYER=RunAsInvoker` pour le lancer sans élévation).

## Conventions

**Rust**
- Erreurs avec `thiserror`, sérialisées pour l'interface : `#[serde(tag = "code", content = "detail")]`. Messages en français, lisibles par l'utilisateur, qui disent quoi faire.
- Un chemin s'affiche avec `path.display()`, jamais `{:?}`.
- Tout `unsafe` porte un commentaire `// SAFETY :` qui justifie l'appel.
- Pas de `unwrap()` sur une donnée externe (sortie d'outil, fichier, disque).
- Seuils du verdict : uniquement dans `Thresholds` (`crates/report`) ou les échelles de `speed.rs`/`age.rs`, jamais en dur dans l'application.

**Application (Tauri)**
- Une commande qui touche un disque, un processus ou le réseau est `async` et passe par `crate::blocking(...)` : une commande synchrone tourne sur le fil principal et gèle l'interface.
- Tâche longue : `jobs.start(...)` avec un identifiant (`surface:/dev/sda`, `speed:/dev/sdb`), progression par `ctx.progress`, annulation par `ctx.cancel`. Vérifier `jobs.is_running` avant de lancer un processus.
- Jamais de shell : on lance des outils connus avec des arguments construits par le code.

**Interface**
- Toute valeur affichée a une info-bulle (`Hint` + texte dans `hints.ts`) qui dit d'où elle vient et comment la lire.
- État qui doit survivre au changement d'écran : `useKept` (`kept.ts`) ou un petit magasin au niveau du module lu avec `useSyncExternalStore`, pas `useState` seul.
- Suivi d'une tâche : `useJob` / `waitForJob` (`jobs.ts`).
- Erreurs : `errorMessage()` (`load.ts`), jamais « [object Object] ».
- Interface compacte (texte de base 13 px, boutons de 32 px) ; couleurs par variables CSS de `styles.css`.

**Tests**
- Toute fonction pure arrive avec ses tests et ses cas limites (vide, absent, bornes, entrée hostile).
- Sorties d'outils : fixtures reconstruites dans `tests/fixtures/`, jamais une sortie réelle d'une machine.
- Un bogue corrigé arrive avec le test qui l'aurait attrapé.

## Branches, pull requests et releases

```
feat/... ──PR squash──► dev ──PR commit de fusion──► master ──► release
            aucune CI          CI : verification.yml         release.yml (builds optimisés)
```

- **Toute branche part de `dev`** et revient dans `dev` par pull request, fusionnée en « squash », branche supprimée (`gh pr merge --squash --delete-branch`). **Aucune CI** sur ces PR : c'est volontaire, pour garder le flux rapide. Avant d'ouvrir la PR, lance toi-même en local les vérifications de la section Commandes.
- **`dev` → `master`** : PR ouverte par l'utilisateur quand il est prêt, fusionnée en **commit de fusion** (jamais en squash : `master` aurait un commit que `dev` ne connaît pas, et chaque PR suivante réappliquerait tout l'historique). La CI `verification.yml` y exige `verif-linux` (format, clippy, tests) et `verif-windows` (clippy, tests), en profil de test non optimisé.
- **Arrivée sur `master`** : `release.yml` construit l'AppImage et le paquet Windows au maximum d'optimisation, en parallèle, sans relancer les tests, puis publie la release (`tools/ci/release.sh`, une par commit) et le wiki.
- `master` et `dev` sont protégées (règles du dépôt, `tools/github/setup-repo.sh protections`) : PR obligatoire, pas de poussée directe, pas de suppression ni de réécriture. Dependabot et AgentFly ouvrent leurs PR vers `dev`.

**Découpe le travail en pull requests empilées** dès qu'une fonctionnalité dépasse une petite modification : une suite de petites PR, chacune basée sur la branche de la précédente, chacune compréhensible seule.

```
dev ◄─ feat/vitesse-1-moteur ◄─ feat/vitesse-2-rapport ◄─ feat/vitesse-3-interface
```

- Découpe par couche ou par étape logique : moteur (crate + tests), puis application et rapport, puis interface. Une PR = un sujet, idéalement moins de 400 lignes modifiées.
- Chaque PR cible la branche de la précédente (`gh pr create --base feat/vitesse-1-moteur`) et dit dans sa description où elle se place dans la pile (« 2/3, après #12 »).
- Fusion du bas vers le haut. **Avant** de fusionner une PR, recible la suivante vers `dev` : si GitHub la recible lui-même à la suppression de la branche fusionnée, il peut la fermer, et une PR fermée dont la branche a été repoussée ne se rouvre plus (il faut la recréer).
  ```
  gh pr edit <PR suivante> --base dev
  gh pr merge <PR du bas> --squash --delete-branch
  git fetch origin
  git rebase --onto origin/dev feat/vitesse-1-moteur feat/vitesse-3-interface --update-refs
  git push --force-with-lease origin feat/vitesse-2-rapport feat/vitesse-3-interface
  ```
  `--update-refs` déplace d'un coup toutes les branches intermédiaires de la pile ; `--onto` retire les commits déjà fusionnés (le squash a créé un autre commit, git ne les reconnaîtrait pas seul).
- Une correction demandée sur une PR du bas se fait sur sa branche, puis se propage vers le haut par rebase.

**Étiquettes de chaque PR** (`gh pr create ... --label amélioration --label disques`), définies dans `tools/github/setup-repo.sh` :
- **un type**, toujours : `bogue` (correction), `amélioration` (nouvelle mesure, écran ou comportement), `documentation` (doc seule), `ci` (CI, releases, configuration du dépôt ou des outils), `dépendances` (mise à jour de dépendances) ; en plus `sécurité` quand la PR touche un risque pour les données ou le système ;
- **les domaines touchés**, zéro ou plusieurs : `disques`, `machine`, `téléphone`, `récupération` ;
- PR `dev` → `master` : les étiquettes de toutes les PR qu'elle livre ;
- jamais `à trier` (issues pas encore examinées) ni `agentflySucettes*` : ces étiquettes sont réservées à AgentFly, et `agentflySucettes` sur une issue **lance** son traitement.

**Jalons (milestones)** : un jalon = une version (`0.2 · Verdict des disques fiable`), livrée par une PR `dev` → `master`. Tu les crées et les tiens à jour :
- toute issue et toute PR vers `dev` est rattachée au jalon de la version qui la livrera (`gh pr create ... --milestone "0.2 · Verdict des disques fiable"`, `gh issue edit 12 --milestone ...`) ; une PR de Dependabot va dans le jalon en cours ;
- un travail prévu mais pas encore planifié : une issue sans jalon ; un sujet qui mérite sa propre version : un nouveau jalon, avec une description qui dit ce qu'il livre (`gh api -X POST repos/Sucettes/10-4PCCheck/milestones -f title=... -f description=...`) ;
- la PR `dev` → `master` qui termine un jalon augmente la version (`Cargo.toml` du workspace, `app/src-tauri/tauri.conf.json`, `app/package.json`) ; une fois fusionnée et la release publiée, ferme le jalon (`gh api -X PATCH repos/Sucettes/10-4PCCheck/milestones/<n> -f state=closed`) ;
- un point de `docs/PLAN.md` (limite connue, question ouverte) qui devient du travail prévu devient une issue dans un jalon.

Messages de commit et titres de PR en français, descriptifs, sans préfixe conventionnel ni émoji : le quoi sur la première ligne, le pourquoi dans le corps.

## AgentFly

Les issues étiquetées `agentflySucettes` sont traitées par AgentFly (voir `agentfly.yml`). Son conteneur n'a ni Windows ni les bibliothèques graphiques de Tauri : il teste le moteur et le typage de l'interface ; la CI du dépôt vérifie le reste sur la pull request. Pas encore de vidéo de démonstration (l'interface a besoin du moteur Tauri pour afficher des données).

## Pièges connus

- `pccheck-app` porte un manifeste `requireAdministrator` : son binaire de test refuse de se lancer sans élévation (erreur 740). D'où `--exclude pccheck-app` en local.
- smartctl : le code de sortie est un masque de bits (voir `RawOutput::check_exit`) ; les attributs ATA sont identifiés par leur nom smartctl, pas seulement leur numéro ; un champ imbriqué absent ne doit pas faire échouer tout le document (`#[serde(default)]`).
- PhotoRec livré sous Windows est un programme Cygwin (guillemets particuliers, voir `sys/cmdline.rs`) ; PhotoRec 7.1 (Debian, Ubuntu) ne connaît pas le format `dovecot` (arguments selon la version détectée).
- Lecture directe d'un SSD ou d'un disque SMR neuf : une zone jamais écrite répond sans être lue ; la vitesse de lecture fiable est la relecture du fichier écrit.
- ConPTY (TestDisk sous Windows) : la sortie ne se ferme qu'à la libération de la session ; la fin est détectée par `try_wait`.
- Fins de ligne : les scripts `.sh` restent en LF (`.gitattributes`), sinon bash échoue sur la CI Windows.
- CI : `release.yml` a un groupe de concurrence par commit (jamais annulé). Un groupe par branche avec `cancel-in-progress` annulait la release d'une fusion dès la fusion suivante.
- Cache Rust : une pull request ne lit que le cache de sa branche de base. `verification.yml` tourne donc aussi sur `master`, sans exécuter les tests (`cargo test --no-run`), pour préparer le cache de la PR `dev` → `master` suivante.
