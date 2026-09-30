#!/usr/bin/env bash
# Configuration du dépôt GitHub, relançable sans risque (chaque étape remplace l'état précédent) :
#   - description, sujets, issues, wiki, fusion des PR (squash pour les PR de fonctionnalité,
#     commit de fusion possible pour garder l'historique d'une grosse branche) ;
#   - fonctions de sécurité (alertes et correctifs Dependabot, détection de secrets, signalement
#     privé de failles) ;
#   - étiquettes des issues ;
#   - règles de protection de master (PR obligatoire et CI verte, ni suppression ni réécriture) ;
#   - pages du wiki (docs/wiki/).
# Prérequis : gh installé et connecté (gh auth login). Les règles de protection et le wiki
# exigent un dépôt public (ou un compte payant).
# Usage, depuis la racine du dépôt (Git Bash) : bash tools/github/setup-repo.sh
set -euo pipefail

repo="Sucettes/10-4PCCheck"
step() { printf '\n== %s\n' "$1"; }

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

step "Sécurité"
gh api -X PUT "repos/$repo/vulnerability-alerts" --silent
gh api -X PUT "repos/$repo/automated-security-fixes" --silent
gh api -X PUT "repos/$repo/private-vulnerability-reporting" --silent
gh api -X PATCH "repos/$repo" --silent --input - <<'JSON'
{ "security_and_analysis": {
    "secret_scanning": { "status": "enabled" },
    "secret_scanning_push_protection": { "status": "enabled" } } }
JSON

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

step "Protection de master"
# Pas d'approbation obligatoire : GitHub interdit d'approuver sa propre PR. La CI fait foi.
ruleset='{
  "name": "Protection de master",
  "target": "branch",
  "enforcement": "active",
  "conditions": { "ref_name": { "include": ["~DEFAULT_BRANCH"], "exclude": [] } },
  "rules": [
    { "type": "deletion" },
    { "type": "non_fast_forward" },
    { "type": "pull_request", "parameters": {
        "required_approving_review_count": 0,
        "dismiss_stale_reviews_on_push": false,
        "require_code_owner_review": false,
        "require_last_push_approval": false,
        "required_review_thread_resolution": true } },
    { "type": "required_status_checks", "parameters": {
        "strict_required_status_checks_policy": false,
        "required_status_checks": [
          { "context": "check" }, { "context": "linux-appimage" }, { "context": "windows" } ] } }
  ]
}'
existing=$(gh api "repos/$repo/rulesets" --jq '.[] | select(.name == "Protection de master") | .id')
if [ -n "$existing" ]; then
  echo "$ruleset" | gh api -X PUT "repos/$repo/rulesets/$existing" --silent --input -
else
  echo "$ruleset" | gh api -X POST "repos/$repo/rulesets" --silent --input -
fi

step "Wiki"
wiki=$(mktemp -d)
if gh repo clone "$repo.wiki" "$wiki" -- -q 2>/dev/null; then
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

step "À faire à la main (pas d'API GitHub)"
echo "Aperçu social : Settings > General > Social preview > Edit > Upload an image,"
echo "fichier docs/social-preview.png (1280 x 640)."
