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
    gh release edit "$tag" --draft=false --latest
    gh release list --limit 100 --json tagName,isDraft \
      --jq '.[] | select(.isDraft) | .tagName' |
      while read -r old; do
        [ "$old" = "$tag" ] || gh release delete "$old" --yes
      done
    ;;
  *)
    echo "usage : $0 create | upload FICHIER... | publish" >&2
    exit 2
    ;;
esac
echo "release $tag : $1 fait"
