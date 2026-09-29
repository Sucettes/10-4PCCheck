#!/usr/bin/env bash
# Construit l'AppImage Linux dans Ubuntu 22.04 (glibc 2.35) via Docker.
# Une AppImage ne tourne que sur des distros dont la glibc est au moins aussi récente que
# celle de la machine de construction : construire sur une base ancienne élargit la compatibilité.
#
# Prérequis sur l'hôte : Docker, Rust (rustup) et Node installés, tools/linux/smartctl présent
# (tools/build-smartctl-linux.sh). Les chaînes d'outils de l'hôte sont montées dans le conteneur.
# Résultat : dist-usb/linux/10-4-pccheck.AppImage
#
# Réseau filtré (proxy) : l'étape finale de linuxdeploy télécharge le runtime AppImage sans passer
# par l'autorité de certification du proxy et échoue. Dans ce cas, fournir le runtime et appimagetool
# téléchargés à la main, et le script termine l'empaquetage lui-même :
#   APPIMAGE_RUNTIME_FILE=/chemin/runtime-x86_64 APPIMAGETOOL=/chemin/appimagetool tools/build-appimage.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NODE_DIR="$(dirname "$(dirname "$(readlink -f "$(command -v node)")")")"
RUSTUP_DIR="${RUSTUP_HOME:-$HOME/.rustup}"
CARGO_DIR="${CARGO_HOME:-$HOME/.cargo}"

[ -x "$ROOT/tools/linux/smartctl" ] || { echo "tools/linux/smartctl manquant : lancer tools/build-smartctl-linux.sh" >&2; exit 1; }

# Proxy et autorité de certification de l'environnement, s'il y en a (réseau d'entreprise, CI).
EXTRA=()
if [ -n "${HTTPS_PROXY:-}" ]; then
  EXTRA+=(-e "HTTPS_PROXY=$HTTPS_PROXY" -e "https_proxy=$HTTPS_PROXY")
fi
if [ -n "${SSL_CERT_FILE:-}" ] && [ -f "$SSL_CERT_FILE" ]; then
  EXTRA+=(-v "$SSL_CERT_FILE:/etc/ssl/certs/extra-ca.crt:ro")
fi

BUNDLE_DIR="$ROOT/target/jammy/release/bundle/appimage"
rm -rf "$BUNDLE_DIR"

set +e
docker run --rm --network host "${EXTRA[@]}" \
  -v "$ROOT:$ROOT" -w "$ROOT/app" \
  -v "$NODE_DIR:$NODE_DIR:ro" \
  -v "$RUSTUP_DIR:/root/.rustup" -v "$CARGO_DIR:/root/.cargo" \
  -e PATH="/root/.cargo/bin:$NODE_DIR/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin" \
  -e CARGO_TARGET_DIR="$ROOT/target/jammy" \
  -e NO_STRIP=true -e APPIMAGE_EXTRACT_AND_RUN=1 \
  ubuntu:22.04 bash -euo pipefail -c '
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq >/dev/null
    apt-get install -y -qq build-essential curl file patchelf libwebkit2gtk-4.1-dev \
      libxdo-dev libssl-dev librsvg2-dev libayatana-appindicator3-dev ca-certificates >/dev/null
    if [ -f /etc/ssl/certs/extra-ca.crt ]; then
      cat /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/extra-ca.crt > /tmp/ca.crt
      export SSL_CERT_FILE=/tmp/ca.crt CARGO_HTTP_CAINFO=/tmp/ca.crt CURL_CA_BUNDLE=/tmp/ca.crt
    fi
    npx tauri build --bundles appimage
  '
BUILD_STATUS=$?
set -e

if [ "$BUILD_STATUS" -ne 0 ]; then
  APPDIR="$(find "$BUNDLE_DIR" -maxdepth 1 -name "*.AppDir" 2>/dev/null | head -1)"
  if [ -n "${APPIMAGE_RUNTIME_FILE:-}" ] && [ -n "${APPIMAGETOOL:-}" ] && [ -n "$APPDIR" ]; then
    echo "Empaquetage final avec le runtime fourni ($APPIMAGE_RUNTIME_FILE)"
    APPIMAGE_EXTRACT_AND_RUN=1 ARCH=x86_64 "$APPIMAGETOOL" --no-appstream \
      --runtime-file "$APPIMAGE_RUNTIME_FILE" "$APPDIR" "$BUNDLE_DIR/10-4-pccheck.AppImage"
  else
    echo "ERREUR : la construction a échoué (code $BUILD_STATUS)" >&2
    exit "$BUILD_STATUS"
  fi
fi

OUT="$ROOT/dist-usb/linux"
mkdir -p "$OUT"
cp "$BUNDLE_DIR"/*.AppImage "$OUT/10-4-pccheck.AppImage"
chmod 755 "$OUT/10-4-pccheck.AppImage"
ls -la "$OUT"
