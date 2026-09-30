# Ubuntu 22.04 avec la base graphique présente sur tout Ubuntu Desktop :
# GTK3 (fontconfig, harfbuzz, X11, Wayland...) et Mesa (libgbm, EGL, GL, GLES, pilotes DRI).
# Ces bibliothèques sont exclues des AppImage par conception (liste d'exclusion AppImage).
# docker build -t pccheck-test/ubuntu-22.04-desktop-libs -f tools/dev/ubuntu-22.04-desktop-libs.Dockerfile tools/dev
FROM ubuntu:22.04
RUN apt-get update -qq \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq libgtk-3-0 libgbm1 libegl1 libgl1 libgles2 libegl-mesa0 libgl1-mesa-dri >/dev/null \
 && rm -rf /var/lib/apt/lists/*
