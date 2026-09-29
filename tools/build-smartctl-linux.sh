#!/usr/bin/env bash
# Compile smartctl en binaire statique (aucune dépendance) pour la clé USB Linux.
# Compilé dans Ubuntu 22.04 via Docker ; le binaire statique tourne sur toute distro x86_64.
# Résultat : tools/linux/smartctl
set -euo pipefail

VERSION="7.5"
# Somme relevée au premier téléchargement (2026-09-29). La vérifier contre la signature
# officielle (smartmontools-7.5.tar.gz.asc) lors d'une montée de version.
SHA256="690b83ca331378da9ea0d9d61008c4b22dde391387b9bbad7f29387f2595f76e"
URL="https://downloads.sourceforge.net/project/smartmontools/smartmontools/${VERSION}/smartmontools-${VERSION}.tar.gz"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

curl -fsSL --retry 3 -o "$WORK/src.tgz" "$URL"
echo "${SHA256}  $WORK/src.tgz" | sha256sum -c -

docker run --rm --network host -v "$WORK:/w" ubuntu:22.04 bash -euo pipefail -c '
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -qq >/dev/null
  apt-get install -y -qq g++ make >/dev/null
  cd /w && tar xzf src.tgz && cd smartmontools-*/
  ./configure --quiet LDFLAGS=-static --without-libsystemd --without-selinux --without-libcap-ng >/dev/null
  make -j"$(nproc)" smartctl >/dev/null
  strip smartctl
  cp smartctl /w/smartctl
'

mkdir -p "$ROOT/tools/linux"
install -m 755 "$WORK/smartctl" "$ROOT/tools/linux/smartctl"
"$ROOT/tools/linux/smartctl" --version | head -1
if ldd "$ROOT/tools/linux/smartctl" >/dev/null 2>&1; then
  echo "ERREUR : le binaire n'est pas statique" >&2
  exit 1
fi
