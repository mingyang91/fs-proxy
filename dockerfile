FROM rust:1.69.0-buster as builder

WORKDIR /app
COPY Cargo.toml /app/Cargo.toml
COPY Cargo.lock /app/Cargo.lock

RUN --mount=type=cache,target=/var/cache/apt \
    apt update && apt install libfuse-dev pkg-config -y

RUN mkdir /app/src
RUN echo "fn main() {println!(\"if you see this, the build broke\")}" > /app/src/main.rs
RUN --mount=target=/usr/local/cargo/registry,type=cache,sharing=locked \
    --mount=target=/app/target,type=cache,sharing=locked \
    cargo fetch
COPY src /app/src

RUN --mount=target=/usr/local/cargo/registry,type=cache,sharing=locked \
    cargo build --release

FROM ubuntu:jammy as runtime

RUN --mount=type=cache,target=/var/cache/apt \
    apt update && apt install fuse -y

COPY --from=builder /app/target/release/fs-proxy /app/fs-proxy
COPY res /app/res
WORKDIR /app
RUN mkdir /mnt/fs-proxy
ENV RUST_LOG=debug
CMD ["./fs-proxy", "--mapping-file", "/app/res/mapping-tree.json", "/mnt/fs-proxy"]