FROM lukemathwalker/cargo-chef:latest-rust-1.89.0 AS chef
WORKDIR /app


FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY apps ./apps
COPY libs ./libs
COPY protos ./protos
RUN cargo chef prepare --recipe-path recipe.json


FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN apt-get update && apt-get install -y protobuf-compiler
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo chef cook --release --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY apps ./apps
COPY libs ./libs
COPY protos ./protos
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release --bin achtung-host


FROM debian:bookworm-slim AS runner
RUN apt-get update && \
    apt-get install -y openssl ca-certificates && \
    rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/achtung-host /usr/local/bin/
ENTRYPOINT ["/usr/local/bin/achtung-host"]

