#!/bin/sh
# Tests Linux du moteur dans un conteneur Docker, dont les tests réels (PhotoRec, The Sleuth Kit,
# TestDisk en pseudo-terminal) avec les versions des paquets Debian (PhotoRec/TestDisk 7.1).
#
# Depuis la racine du dépôt (Windows : Git Bash, avec MSYS_NO_PATHCONV=1) :
#   docker run --rm -v "$PWD:/src:ro" -v pccheck-target:/target \
#     -v pccheck-cargo:/usr/local/cargo/registry -v pccheck-work:/work \
#     rust:1-bookworm sh /src/tools/dev/docker-linux-tests.sh
#
# /work doit être un volume Docker (ext4 sur un vrai disque) : la récupération refuse une
# destination sur l'overlay du conteneur, dont elle ne peut pas vérifier le disque.
set -e
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq >/dev/null
apt-get install -y -qq sleuthkit testdisk python3 >/dev/null 2>&1
rustup component add clippy >/dev/null 2>&1
mkdir -p /work/tmp
export TMPDIR=/work/tmp CARGO_TARGET_DIR=/target
cd /src

echo "== tests du moteur"
cargo test --workspace --exclude pccheck-app --no-fail-fast 2>&1 | grep -E "test result|FAILED|panicked|^error" || true
echo "== clippy"
cargo clippy --workspace --exclude pccheck-app --all-targets 2>&1 | grep -E "^(warning|error)|-->" || true

echo "== tests réels"
python3 tools/dev/make-photorec-image.py /work/image.dd
python3 tools/dev/make-fat-image.py /work/fat.img
export PCCHECK_PHOTOREC="$(command -v photorec)" PCCHECK_TESTDISK="$(command -v testdisk)"
export PCCHECK_TSK="$(command -v tsk_recover)"
export PCCHECK_TEST_IMAGE=/work/image.dd PCCHECK_TEST_FAT=/work/fat.img
cargo test -p pccheck-recovery --no-fail-fast --test real_photorec --test real_tsk --test real_testdisk \
  -- --ignored --test-threads=1 2>&1 | grep -E "^test |test result|panicked" || true
