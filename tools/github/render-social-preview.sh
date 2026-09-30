#!/usr/bin/env bash
# Rend docs/social-preview.svg en PNG 1280 x 640 (aperçu social du dépôt), dans un conteneur
# Debian : rien n'est installé sur la machine. Police : IBM Plex Sans de l'application
# (app/node_modules/@fontsource, installé par « npm ci » dans app/).
# Usage, depuis la racine du dépôt (Windows : Git Bash, avec MSYS_NO_PATHCONV=1) :
#   tools/github/render-social-preview.sh
set -euo pipefail
fonts="$PWD/app/node_modules/@fontsource/ibm-plex-sans/files"
[ -d "$fonts" ] || { echo "police absente : lancer npm ci dans app/" >&2; exit 1; }
docker run --rm -v "$PWD/docs:/docs" -v "$fonts:/fonts:ro" debian:bookworm-slim sh -c '
  apt-get update -qq >/dev/null &&
  apt-get install -y -qq librsvg2-bin fontconfig >/dev/null 2>&1 &&
  mkdir -p /usr/share/fonts/plex &&
  cp /fonts/ibm-plex-sans-latin-*-normal.woff /usr/share/fonts/plex/ &&
  fc-cache -f >/dev/null &&
  fc-list | grep -c "IBM Plex Sans" &&
  rsvg-convert -w 1280 -h 640 /docs/social-preview.svg -o /docs/social-preview.png &&
  ls -l /docs/social-preview.png'
