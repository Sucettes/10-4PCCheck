#!/usr/bin/env bash
# Lance l'app sur un écran virtuel (Xvfb), attend le rendu et enregistre une capture.
# Usage : tools/dev/screenshot-linux.sh <commande de l'app> <capture.png> [secondes]
# Exemple : PCCHECK_SMARTCTL=tools/dev/fake-smartctl tools/dev/screenshot-linux.sh target/release/pccheck /tmp/app.png
set -euo pipefail

APP="$1"
OUT="$2"
WAIT="${3:-8}"
DISPLAY_NUM=":$((90 + RANDOM % 9))"

Xvfb "$DISPLAY_NUM" -screen 0 1280x820x24 -nolisten tcp &
XVFB_PID=$!
cleanup() {
  kill "${APP_PID:-}" 2>/dev/null || true
  kill "$XVFB_PID" 2>/dev/null || true
}
trap cleanup EXIT
sleep 1

DISPLAY="$DISPLAY_NUM" "$APP" &
APP_PID=$!
sleep "$WAIT"

if ! kill -0 "$APP_PID" 2>/dev/null; then
  echo "ERREUR : l'application s'est arrêtée avant la capture" >&2
  wait "$APP_PID" || true
  exit 1
fi
DISPLAY="$DISPLAY_NUM" import -window root "$OUT"
echo "capture : $OUT"
