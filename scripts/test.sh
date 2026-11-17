#!/usr/bin/env bash
# Native tests of the engine's Move port and the performer (in the build image).
#   ./scripts/test.sh              chimera-move's tests
#   ./scripts/test.sh --engine     also the whole engine suite (the exact path must be unchanged)
set -e
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PKGS="-p chimera-move"
[ "$1" = "--engine" ] && PKGS="-p chimera-move -p chimera"
docker run --rm -v "$REPO_ROOT:/build" -u "$(id -u):$(id -g)" -w /build/engine \
    -e CARGO_HOME=/build/build/cargo-home -e CARGO_TARGET_DIR=/build/build/cargo-native \
    chimera-builder cargo test --release $PKGS -- --test-threads=1 --nocapture
