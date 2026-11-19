# CHIMERA module build environment. Target: Ableton Move (aarch64 Linux).
#
# Ubuntu 22.04 on purpose (as the host's own build and ICHI-1's): its GCC 11 runtime, linked
# statically into dsp.so, references no glibc symbol newer than a stock Move provides (Debian
# bookworm's GCC 12 runtime pulls in 2.35/2.36 symbols). Rust's aarch64-unknown-linux-gnu std
# targets glibc 2.17.
FROM ubuntu:22.04

# gcc + libc6-dev: the native compiler cargo's build scripts and the native tests link with. An
# arm64 host gets it with the cross compiler; an x86 one (the release CI) does not.
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y \
    gcc libc6-dev \
    gcc-aarch64-linux-gnu \
    binutils-aarch64-linux-gnu \
    libc6-dev-arm64-cross \
    curl ca-certificates \
    file \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.91.0 \
    && rustup target add aarch64-unknown-linux-gnu \
    && chmod -R a+rwX /opt/cargo /opt/rustup

ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    CROSS_PREFIX=aarch64-linux-gnu-

WORKDIR /build
