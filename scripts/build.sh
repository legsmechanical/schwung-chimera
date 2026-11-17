#!/usr/bin/env bash
# Build the CHIMERA module for Schwung (Move, aarch64).
#
# Host side first (needs node): regenerate module.json, the plugin's param header and the Rust
# ids from src/params.json. Then cross-compile in Docker. Set CROSS_PREFIX to skip Docker.
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
# This module's OWN builder tag: move-anything-builder is shared by other repos with different
# Dockerfiles, and rebuilding it here would break theirs.
IMAGE_NAME="chimera-builder"
ID="chimera"

if [ -z "$CROSS_PREFIX" ] && [ ! -f "/.dockerenv" ]; then
    echo "=== CHIMERA build (via Docker) ==="
    command -v node >/dev/null || { echo "ERROR: node is required (generator)"; exit 1; }
    node "$REPO_ROOT/tools/gen.mjs"
    if ! docker image inspect "$IMAGE_NAME" &>/dev/null; then
        echo "Building Docker image (first time only)..."
        docker build -t "$IMAGE_NAME" -f "$SCRIPT_DIR/Dockerfile" "$SCRIPT_DIR"
    fi
    docker run --rm -v "$REPO_ROOT:/build" -u "$(id -u):$(id -g)" -w /build "$IMAGE_NAME" ./scripts/build.sh
    echo "=== Done ==="
    exit 0
fi

CROSS_PREFIX="${CROSS_PREFIX:-aarch64-linux-gnu-}"
cd "$REPO_ROOT"
echo "=== Cross-compiling CHIMERA ($CROSS_PREFIX) ==="

# Everything in dist/ is regenerated: anything surviving would be stale.
rm -rf "dist/$ID" build/arm
mkdir -p build/arm "dist/$ID"

# the engine + performer, as a static library with a C ABI. Crate downloads are kept in build/
# (the image's own CARGO_HOME belongs to root; the toolchain is still found on PATH).
export CARGO_HOME="$REPO_ROOT/build/cargo-home"
( cd engine && CARGO_TARGET_DIR="$REPO_ROOT/build/cargo" cargo build --release --locked \
    --target aarch64-unknown-linux-gnu -p chimera-move )
LIB="build/cargo/aarch64-unknown-linux-gnu/release/libchimera_move.a"

# -fvisibility=hidden + the version script: only move_plugin_init_v2 is exported, so nothing of
#   ours (Rust's std included) can interpose on, or be interposed by, the host's symbols.
# -static-libgcc: the unwinder travels inside dsp.so (a stock Move's runtime is older).
# --gc-sections: the engine's CLI-only code (analysis, calibration) is dropped.
"${CROSS_PREFIX}gcc" -std=gnu11 -O2 -fPIC -shared -fvisibility=hidden \
    -Wall -Wextra -Wno-unused-parameter \
    src/dsp/chimera_plugin.c "$LIB" \
    -o build/arm/dsp.so -Isrc/dsp \
    -static-libgcc -pthread -ldl -lm \
    -Wl,--version-script=src/dsp/exports.map -Wl,--exclude-libs,ALL -Wl,--gc-sections
"${CROSS_PREFIX}strip" --strip-unneeded build/arm/dsp.so

echo "Packaging..."
cp src/module.json src/ui.js src/help.json "dist/$ID/"
cp build/arm/dsp.so "dist/$ID/dsp.so"
cp engine/public/data/cmudict.txt.gz "dist/$ID/cmudict.txt.gz"
cp LICENSE "dist/$ID/LICENSE"
cp engine/public/data/CMUDICT-LICENSE.txt "dist/$ID/CMUDICT-LICENSE.txt"

# Release checks: nothing dynamic beyond libc/libm/libdl/libpthread, no glibc symbol newer than
# a stock Move provides, and one exported function.
NEEDED="$("${CROSS_PREFIX}readelf" -d build/arm/dsp.so | grep NEEDED || true)"
echo "$NEEDED"
if echo "$NEEDED" | grep -qE "libstdc\+\+|libgcc_s"; then echo "ERROR: dynamic C/C++ runtime"; exit 1; fi
GLIBC_MAX="$("${CROSS_PREFIX}objdump" -T build/arm/dsp.so | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -1)"
echo "newest glibc symbol: $GLIBC_MAX"
if [ "$(printf '%s\nGLIBC_2.34\n' "$GLIBC_MAX" | sort -V | tail -1)" != "GLIBC_2.34" ]; then
    echo "ERROR: needs $GLIBC_MAX, newer than a stock Move provides"; exit 1
fi
EXPORTS="$("${CROSS_PREFIX}objdump" -T build/arm/dsp.so | grep ' DF .text' | awk '{print $NF}' | tr '\n' ' ')"
echo "exported: $EXPORTS"
[ "$(echo $EXPORTS)" = "move_plugin_init_v2" ] || { echo "ERROR: unexpected exports"; exit 1; }
ls -la build/arm/dsp.so

cd dist
tar -czf "$ID-module.tar.gz" "$ID/"
cd ..
echo "=== Build complete: dist/$ID/ + dist/$ID-module.tar.gz ==="
