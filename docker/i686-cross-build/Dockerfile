# taskboard i686 cross-build image — the D250 leg of the kiosk bring-up
# (docs/kiosk_bringup_runbook.org §5: the artifact must link a
# glibc-matched userland, so the build runs in debian:12, not on the
# dev host).
#
# Build:  docker build -t taskboard-build:debian12-i686 docker/i686-cross-build/
# Invoke: scripts/i686-cross-build.sh
#
# Layer notes (each was a real bring-up failure on 2026-10-21):
# - libc6-dev is REQUIRED for the host arch: Debian's gcc only
#   recommends it, and without it the host build scripts cannot link
#   (rust-lld: cannot open Scrt1.o / crti.o).
# - the i386 dev-library set covers every pkg-config consumer in the
#   dependency graph (fontconfig, dbus, udev, seat, drm/gbm, wayland,
#   xkbcommon, libinput); yeslogic-fontconfig-sys and libdbus-sys fail
#   their build scripts without it.
# - /opt/rustup and /opt/cargo are chmod'd world-writable so the image
#   can be run as the host uid (-u) while rustup resolves the
#   workspace's `channel = "stable"` toolchain pin.
FROM debian:12
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo
ENV PATH=/opt/cargo/bin:$PATH
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl gcc libc6-dev gcc-i686-linux-gnu pkg-config git \
 && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --profile minimal --default-toolchain stable \
 && rustup target add i686-unknown-linux-gnu \
 && chmod -R a+rwX /opt/rustup /opt/cargo
RUN dpkg --add-architecture i386 \
 && apt-get update \
 && apt-get install -y --no-install-recommends \
      libfontconfig1-dev:i386 libdbus-1-dev:i386 libudev-dev:i386 \
      libseat-dev:i386 libdrm-dev:i386 libgbm-dev:i386 \
      libwayland-dev:i386 libxkbcommon-dev:i386 libinput-dev:i386
