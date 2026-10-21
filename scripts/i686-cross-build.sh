#!/usr/bin/env bash
# i686 cross-build lane for the D250 kiosk leg (kiosk bring-up runbook
# §5): build taskboard-app for i686-unknown-linux-gnu inside a
# debian:12 container so the artifact links the target's glibc (the
# dev host's newer glibc produces a binary the D250 refuses to load).
#
# Prerequisites (one-time):
#   docker build -t taskboard-build:debian12-i686 docker/i686-cross-build/
#
# The build cache lives in "$ROOT/target-i686" (a sibling of the
# checkout, outside the workspace's target dir) and the cargo registry
# in a throwaway /tmp dir inside the container; first run compiles the
# whole dependency tree (~15 min), later runs only the changed crates.
#
# Output: target-i686/i686-unknown-linux-gnu/release/taskboard
#
# WARNING: do not run this while other rounds are committing to the
# checkout — a container build that races a mid-air commit can compile
# a stale-schema binary (finding F8,
# .artifacts/reports/2026-10-21-netbook-kiosk-test-environment-report.md).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE="taskboard-build:debian12-i686"
TARGET_DIR="${ROOT}/target-i686"

mkdir -p "$TARGET_DIR"

# PKG_CONFIG_* points pkg-config at the i386 sysroot for the target's
# C dependencies; the CARGO_TARGET_* / CC_* pair is runbook §5.2's
# cross-linker (mandatory — plain `cc -m32` picks x86_64 gcc libs).
exec docker run --rm \
    -u "$(id -u):$(id -g)" \
    -e CARGO_HOME=/tmp/cargo-home \
    -e CARGO_TARGET_DIR=/target \
    -e CARGO_TARGET_I686_UNKNOWN_LINUX_GNU_LINKER=i686-linux-gnu-gcc \
    -e CC_i686_unknown_linux_gnu=i686-linux-gnu-gcc \
    -e PKG_CONFIG_ALLOW_CROSS=1 \
    -e PKG_CONFIG_LIBDIR=/usr/lib/i386-linux-gnu/pkgconfig:/usr/share/pkgconfig \
    -v "$ROOT":/work:rw \
    -v "$TARGET_DIR":/target \
    -w /work \
    "$IMAGE" \
    cargo build --release --target i686-unknown-linux-gnu -p taskboard-app "$@"
