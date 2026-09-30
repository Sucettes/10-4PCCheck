#!/usr/bin/env bash
# Lance l'AppImage dans des conteneurs de distros différentes, sans aucun paquet ajouté,
# en autotest puis pour une capture d'écran. L'écran virtuel (Xvfb) tourne sur l'hôte.
# Usage : tools/dev/test-appimage-distros.sh <AppImage> <dossier de sortie>
set -euo pipefail

APPIMAGE="$(readlink -f "$1")"
OUT="$(mkdir -p "$2" && cd "$2" && pwd)"
# Images proches d'un bureau réel. Les images minimales (ubuntu:22.04, fedora:41) n'ont pas
# fontconfig, X11 ni freetype, que la liste d'exclusion AppImage suppose présents sur tout bureau.
#  - pccheck-test/ubuntu-22.04-desktop-libs : ubuntu:22.04 + GTK3 et Mesa (tools/dev/ubuntu-22.04-desktop-libs.Dockerfile)
#  - linuxserver/webtop:fedora-xfce : Fedora avec bureau XFCE complet
#  - linuxmintd/mint22-amd64 : Linux Mint 22
read -r -a IMAGES <<< "${IMAGES:-pccheck-test/ubuntu-22.04-desktop-libs linuxserver/webtop:fedora-xfce linuxmintd/mint22-amd64}"
DISP=":97"

Xvfb "$DISP" -screen 0 1280x820x24 -nolisten tcp >/dev/null 2>&1 &
XVFB_PID=$!
trap 'kill $XVFB_PID 2>/dev/null || true' EXIT
sleep 1

run_in() {
  local image="$1"; shift
  # APPIMAGE_EXTRACT_AND_RUN : pas de FUSE dans un conteneur.
  # Délai appliqué côté hôte : le `timeout` de certaines images (Fedora 44) échoue en conteneur.
  timeout 150 docker run --rm --entrypoint "" -e DISPLAY="$DISP" -e APPIMAGE_EXTRACT_AND_RUN=1 \
    -v /tmp/.X11-unix:/tmp/.X11-unix -v "$APPIMAGE:/app/PCCheck.AppImage:ro" -v "$OUT:/out" \
    "$image" "$@"
}

status=0
for image in "${IMAGES[@]}"; do
  tag="$(echo "$image" | tr '/:' '__')"
  echo "== $image"
  if run_in "$image" /app/PCCheck.AppImage --self-test "/out/selftest-$tag.json" > "$OUT/log-$tag.txt" 2>&1 \
     && grep -q '"ok": true' "$OUT/selftest-$tag.json" && grep -q '"status": "ready"' "$OUT/selftest-$tag.json"; then
    echo "   autotest : OK"
  else
    echo "   autotest : ÉCHEC (voir $OUT/log-$tag.txt)"; status=1
  fi

  # Capture : l'app tourne en arrière-plan le temps de la prise de vue.
  cid="$(docker run -d --rm --entrypoint "" -e DISPLAY="$DISP" -e APPIMAGE_EXTRACT_AND_RUN=1 \
    -v /tmp/.X11-unix:/tmp/.X11-unix -v "$APPIMAGE:/app/PCCheck.AppImage:ro" \
    "$image" /app/PCCheck.AppImage)"
  sleep 12
  DISPLAY="$DISP" import -window root "$OUT/screenshot-$tag.png"
  docker kill "$cid" >/dev/null 2>&1 || true
  echo "   capture : $OUT/screenshot-$tag.png"
done
exit $status
