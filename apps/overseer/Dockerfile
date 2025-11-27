FROM lukemathwalker/cargo-chef:latest-rust-1.89.0 AS chef
WORKDIR /app


FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY apps ./apps
COPY libs ./libs
RUN cargo chef prepare --recipe-path recipe.json --bin overseer


FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN apt-get update && apt-get install -y protobuf-compiler
RUN cargo chef cook --release --recipe-path recipe.json --bin overseer
COPY Cargo.toml Cargo.lock ./
COPY apps ./apps
COPY libs ./libs
COPY protos ./protos
RUN cargo build --release --bin overseer


FROM debian:bookworm-slim AS runner
RUN apt-get update && \
    apt-get install -y openssl ca-certificates && \
    rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/overseer /usr/local/bin/
ENTRYPOINT ["/usr/local/bin/overseer"]
