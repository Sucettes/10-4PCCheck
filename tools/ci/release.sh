#!/usr/bin/env bash
# Release GitHub d'un build de master, en trois temps (voir .github/workflows/release.yml) :
#   release.sh create              brouillon créé avant les builds (job brouillon)
#   release.sh upload FICHIER...   fichiers joints par les jobs de build
#   release.sh publish             brouillon publié quand les deux builds ont réussi
# Un build qui échoue ne publie donc jamais de release incomplète.
# Nom : v<version de Cargo.toml>-<commit court>, ex. v0.1.0-86df60b : une release par commit de
# master, et une relance du workflow sur le même commit reprend le même brouillon.
set -euo pipefail

version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
short=${GITHUB_SHA::7}
tag="v${version}-${short}"

case "${1:-}" in
  create)
    if ! gh release view "$tag" >/dev/null 2>&1; then
      gh release create "$tag" --draft --target "$GITHUB_SHA" \
        --title "PCCheck $version · $(date -u +%Y-%m-%d) · $short" \
        --generate-notes
    fi
    ;;
  upload)
    shift
    gh release upload "$tag" "$@" --clobber
    ;;
  publish)
    # Plusieurs builds peuvent tourner en même temps (fusions rapprochées) et finir dans le
    # désordre : « latest » va seulement à la release du dernier commit de master.
    head=$(gh api "repos/$GITHUB_REPOSITORY/commits/master" --jq .sha)
    if [ "$head" = "$GITHUB_SHA" ]; then
      gh release edit "$tag" --draft=false --latest
    else
      gh release edit "$tag" --draft=false --latest=false
    fi
    # Brouillons abandonnés (build échoué ou annulé) : supprimés, sauf ceux d'un build encore en
    # cours, qui seront publiés à leur fin.
    active=$(gh api "repos/$GITHUB_REPOSITORY/actions/workflows/release.yml/runs?per_page=50" \
      --jq '.workflow_runs[] | select(.status != "completed") | .head_sha[0:7]')
    gh release list --limit 100 --json tagName,isDraft \
      --jq '.[] | select(.isDraft) | .tagName' |
      while read -r old; do
        [ "$old" = "$tag" ] && continue
        grep -qxF "${old##*-}" <<<"$active" && continue
        gh release delete "$old" --yes
      done
    ;;
  *)
    echo "usage : $0 create | upload FICHIER... | publish" >&2
    exit 2
    ;;
esac
echo "release $tag : $1 fait"
