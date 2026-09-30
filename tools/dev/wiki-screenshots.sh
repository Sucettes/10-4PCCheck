#!/usr/bin/env bash
# Régénère les captures d'écran du wiki (docs/wiki/images/) à partir du mode démo : données
# fictives, jamais celles de la machine. Construit l'interface, puis la photographie avec
# Playwright dans un conteneur (rien d'installé sur la machine).
# Usage, depuis la racine du dépôt (Windows : Git Bash, avec MSYS_NO_PATHCONV=1) :
#   tools/dev/wiki-screenshots.sh
set -euo pipefail
PLAYWRIGHT=1.63.0

npm run build --prefix app
mkdir -p docs/wiki/images
docker run --rm \
  -v "$PWD/app/dist:/dist:ro" \
  -v "$PWD/docs/wiki/images:/out" \
  -v "$PWD/tools/dev/wiki-screenshots.mjs:/work/shots.mjs:ro" \
  "mcr.microsoft.com/playwright:v${PLAYWRIGHT}-noble" \
  sh -c "cd /work && npm init -y >/dev/null && npm i -s playwright@${PLAYWRIGHT} >/dev/null && node shots.mjs"
