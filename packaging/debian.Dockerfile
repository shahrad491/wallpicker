# syntax=docker/dockerfile:1
# Opt-in build recipe. Nothing is pulled, built, or installed by make package.
# Override with an official Rust image on the oldest Debian release you support.
ARG RUST_IMAGE=rust:1-bookworm
FROM ${RUST_IMAGE} AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential pkg-config dpkg-dev binutils ca-certificates \
        libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev \
        libx11-dev libx11-xcb-dev libxcb1-dev libxcursor-dev libxi-dev libxrandr-dev \
        libegl1-mesa-dev libvulkan-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .
ARG DEB_MAINTAINER="wallpicker contributors"
ENV DEB_MAINTAINER=${DEB_MAINTAINER}
# Cargo.lock stays in the build context; no host target/ or binary is copied.
RUN cargo build --release --locked \
    && ./scripts/package.sh --skip-build --format deb --output-dir /out \
    && ./scripts/package.sh --skip-build --format tar --output-dir /out

# Local exporter returns only artifacts, not a runnable application container.
FROM scratch AS artifacts
COPY --from=builder /out/ /
