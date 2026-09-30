#!/usr/bin/env bash
# Configuration du dépôt GitHub, relançable sans risque (chaque étape remplace l'état précédent) :
#   - description, sujets, issues, wiki, fusion des PR (squash pour les PR de fonctionnalité,
#     commit de fusion possible pour garder l'historique d'une grosse branche) ;
#   - fonctions de sécurité (alertes et correctifs Dependabot, détection de secrets, signalement
#     privé de failles) ;
#   - étiquettes des issues ;
#   - branche dev et règles de protection (master : PR depuis dev, commit de fusion, CI verte ;
#     dev : PR en squash, sans CI ; ni suppression ni réécriture) ;
#   - pages du wiki (docs/wiki/).
# Prérequis : gh installé et connecté (gh auth login). Les règles de protection et le wiki
# exigent un dépôt public (ou un compte payant).
# Usage, depuis la racine du dépôt (Git Bash) : bash tools/github/setup-repo.sh [étape]
# Sans argument, toutes les étapes ; sinon une seule : depot, securite, etiquettes, protections, wiki.
set -euo pipefail

repo="Sucettes/10-4PCCheck"
only="${1:-}"
step() { printf '\n== %s\n' "$1"; }
# Étape demandée, ou toutes sans argument (« main » : rappel des réglages sans API).
want() { [ -z "$only" ] || [ "$only" = "$1" ]; }

if want depot; then
step "Description, sujets, issues, wiki, fusion"
gh repo edit "$repo" \
  --description "Diagnostic portable d'occasion sur clé USB : ordinateurs, disques et téléphones Android. Santé SMART, âge, vitesse, tests de charge, récupération de fichiers et rapport PDF avec verdict. Windows et Linux." \
  --homepage "https://github.com/$repo/wiki" \
  --enable-issues --enable-wiki --enable-projects=false \
  --enable-squash-merge --enable-merge-commit --enable-rebase-merge=false \
  --delete-branch-on-merge --enable-auto-merge
for topic in diagnostic hardware smart smartctl disk-health ssd hdd usb data-recovery \
  photorec testdisk sleuthkit android adb tauri rust react windows linux; do
  gh repo edit "$repo" --add-topic "$topic" >/dev/null
done
fi

if want securite; then
step "Sécurité"
gh api -X PUT "repos/$repo/vulnerability-alerts" --silent
gh api -X PUT "repos/$repo/automated-security-fixes" --silent
gh api -X PUT "repos/$repo/private-vulnerability-reporting" --silent
gh api -X PATCH "repos/$repo" --silent --input - <<'JSON'
{ "security_and_analysis": {
    "secret_scanning": { "status": "enabled" },
    "secret_scanning_push_protection": { "status": "enabled" } } }
JSON
fi

if want etiquettes; then
step "Étiquettes"
label() { gh label create "$1" --repo "$repo" --color "$2" --description "$3" --force; }
label "bogue" "d73a4a" "Quelque chose ne fonctionne pas ou donne un résultat faux"
label "amélioration" "a2eeef" "Nouvelle mesure, écran ou comportement"
label "à trier" "fbca04" "Pas encore examiné"
label "sécurité" "b60205" "Risque pour les données ou le système"
label "documentation" "0075ca" "Wiki, README, textes de l'interface"
label "dépendances" "0366d6" "Mises à jour proposées par Dependabot"
label "disques" "5319e7" "SMART, vitesse, scan de surface, capacité"
label "récupération" "1d76db" "PhotoRec, The Sleuth Kit, TestDisk"
label "téléphone" "0e8a16" "Analyse Android par adb"
# AgentFly (issues → pull requests) : étiquette de l'instance (AGENTFLY_LABEL) et étiquette
# posée pendant le traitement d'une issue.
label "agentflySucettes" "7057ff" "À traiter par AgentFly"
label "agentflySucettes:in-progress" "c5def5" "En cours de traitement par AgentFly"
fi

if want protections; then
step "Protection des branches"
# Flux : feat/... → dev (squash, aucune CI) → dev → master (commit de fusion, CI verte) → release.
# Pas d'approbation obligatoire : GitHub interdit d'approuver sa propre PR.
# Commit de fusion seulement vers master : un squash créerait sur master un commit que dev ne
# connaît pas, et chaque PR suivante de dev réappliquerait tout l'historique déjà fusionné.
protect() { # nom, référence, méthodes de fusion (JSON), vérifications requises (JSON)
  local checks=""
  if [ "$4" != "[]" ]; then
    checks=',
    { "type": "required_status_checks", "parameters": {
        "strict_required_status_checks_policy": false,
        "required_status_checks": '"$4"' } }'
  fi
  local ruleset='{
  "name": "'"$1"'",
  "target": "branch",
  "enforcement": "active",
  "conditions": { "ref_name": { "include": ["'"$2"'"], "exclude": [] } },
  "rules": [
    { "type": "deletion" },
    { "type": "non_fast_forward" },
    { "type": "pull_request", "parameters": {
        "required_approving_review_count": 0,
        "dismiss_stale_reviews_on_push": false,
        "require_code_owner_review": false,
        "require_last_push_approval": false,
        "required_review_thread_resolution": true,
        "allowed_merge_methods": '"$3"' } }'"$checks"'
  ]
}'
  local existing
  existing=$(gh api "repos/$repo/rulesets" --jq ".[] | select(.name == \"$1\") | .id")
  if [ -n "$existing" ]; then
    echo "$ruleset" | gh api -X PUT "repos/$repo/rulesets/$existing" --silent --input -
  else
    echo "$ruleset" | gh api -X POST "repos/$repo/rulesets" --silent --input -
  fi
  echo "$1 : appliquée"
}
git ls-remote --exit-code --heads "https://github.com/$repo.git" dev >/dev/null ||
  gh api -X POST "repos/$repo/git/refs" --silent \
    -f ref=refs/heads/dev -f sha="$(gh api "repos/$repo/commits/master" --jq .sha)"
protect "Protection de master" "~DEFAULT_BRANCH" '["merge"]' \
  '[{ "context": "verif-linux" }, { "context": "verif-windows" }]'
protect "Protection de dev" "refs/heads/dev" '["squash"]' '[]'
fi

if want wiki; then
step "Wiki"
wiki=$(mktemp -d)
# HTTPS, comme le dépôt : le protocole de gh (souvent SSH) exige une clé SSH configurée.
if git clone -q "https://github.com/$repo.wiki.git" "$wiki" 2>/dev/null; then
  cp docs/wiki/*.md "$wiki/"
  git -C "$wiki" add -A
  if git -C "$wiki" diff --cached --quiet; then
    echo "wiki déjà à jour"
  else
    git -C "$wiki" commit -q -m "Documentation (docs/wiki du dépôt)"
    git -C "$wiki" push -q
    echo "wiki publié"
  fi
else
  echo "Wiki pas encore créé : ouvre https://github.com/$repo/wiki, clique « Create the first page »,"
  echo "enregistre-la telle quelle, puis relance ce script (elle sera remplacée)."
fi
rm -rf "$wiki"
fi

if want main; then
step "À faire à la main (pas d'API GitHub)"
echo "Aperçu social : Settings > General > Social preview > Edit > Upload an image,"
echo "fichier docs/social-preview.png (1280 x 640)."
fi
