#!/usr/bin/env bash
# Release GitHub d'un build de master, en trois temps (voir .github/workflows/ci.yml) :
#   release.sh create        brouillon créé après les tests (job check)
#   release.sh upload FICHIER...   fichiers joints par les jobs de build
#   release.sh publish       brouillon publié quand tous les builds ont réussi ; les brouillons
#                            laissés par des builds échoués ou annulés sont supprimés.
# Un build qui échoue ne publie donc jamais de release incomplète.
# Nom : v<version de Cargo.toml>-build.<numéro d'exécution de la CI>, ex. v0.1.0-build.42.
set -euo pipefail

version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
tag="v${version}-build.${GITHUB_RUN_NUMBER}"

case "${1:-}" in
  create)
    # Relance d'un job : le brouillon existe déjà.
    if ! gh release view "$tag" >/dev/null 2>&1; then
      gh release create "$tag" --draft --target "$GITHUB_SHA" \
        --title "PCCheck $version · build $GITHUB_RUN_NUMBER" \
        --generate-notes
    fi
    ;;
  upload)
    shift
    gh release upload "$tag" "$@" --clobber
    ;;
  publish)
    # Plusieurs builds de master peuvent tourner en même temps (fusions rapprochées) et finir
    # dans le désordre : « latest » va toujours au numéro de build le plus grand.
    gh release edit "$tag" --draft=false --latest=false
    newest=$(gh release list --exclude-drafts --limit 100 --json tagName --jq '.[].tagName' |
      sed -n 's/.*-build\.\([0-9][0-9]*\)$/\1/p' | sort -n | tail -1)
    if [ "$newest" = "$GITHUB_RUN_NUMBER" ]; then
      gh release edit "$tag" --latest
    fi
    # Brouillons abandonnés : seulement ceux dont le build est terminé (échoué ou annulé). Celui
    # d'un build encore en cours est gardé, il sera publié à sa fin.
    gh release list --limit 100 --json tagName,isDraft \
      --jq '.[] | select(.isDraft) | .tagName' |
      while read -r old; do
        [ "$old" = "$tag" ] && continue
        number=${old##*-build.}
        case "$number" in '' | *[!0-9]*) continue ;; esac
        status=$(gh api "repos/$GITHUB_REPOSITORY/actions/workflows/ci.yml/runs?per_page=100" \
          --jq ".workflow_runs[] | select(.run_number == $number) | .status" | head -1)
        [ "$status" = "completed" ] || [ -z "$status" ] || continue
        gh release delete "$old" --yes
      done
    ;;
  *)
    echo "usage : $0 create | upload FICHIER... | publish" >&2
    exit 2
    ;;
esac
echo "release $tag : $1 fait"
