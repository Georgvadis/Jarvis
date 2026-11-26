FROM rust:bookworm

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl xz-utils cmake ninja-build python3 ca-certificates \
    && curl -fsSL https://deb.nodesource.com/setup_24.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && curl -fsSL https://ziglang.org/download/0.15.2/zig-x86_64-linux-0.15.2.tar.xz \
       | tar -xJ -C /usr/local --strip-components=1 \
    && rustup target add aarch64-unknown-linux-musl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
