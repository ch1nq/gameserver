FROM lukemathwalker/cargo-chef:latest-rust-1.89.0 AS chef
WORKDIR /app


FROM chef AS planner
COPY apps/website /app
RUN cargo chef prepare --recipe-path recipe.json


FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN apt-get update && apt-get install -y protobuf-compiler
RUN cargo chef cook --release --recipe-path recipe.json
COPY apps/website /app
COPY protos/ /protos
RUN cargo build --release --bin website


FROM debian:bookworm-slim AS runner
WORKDIR /app
RUN apt-get update && \
    apt-get install -y openssl ca-certificates && \
    rm -rf /var/lib/apt/lists/*
COPY apps/website/static /app/static
COPY apps/website/migrations /app/migrations
COPY --from=builder /app/target/release/website /usr/local/bin/
ENTRYPOINT ["/usr/local/bin/website"]
