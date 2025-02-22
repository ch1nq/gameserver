FROM lukemathwalker/cargo-chef:latest-rust-1.84.1 AS chef
WORKDIR /app


FROM chef AS planner
COPY apps/website /app
RUN cargo chef prepare --recipe-path recipe.json


FROM chef AS builder 
RUN apt-get update && \
    apt-get install -y --no-install-recommends curl npm libc6-dev clang
RUN npm install -g sass
RUN cargo install --locked cargo-leptos --version=0.2.26
RUN rustup target add wasm32-unknown-unknown

COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo chef cook --release --recipe-path recipe.json --target wasm32-unknown-unknown

    # RUN --mount=type=cache,target=/usr/local/cargo/registry \
#     --mount=type=cache,target=/usr/local/cargo/git \
#     cargo chef cook --release --recipe-path recipe.json --target x86_64-unknown-linux-gnu
# RUN cargo build --release --bin app

COPY apps/website /app
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo leptos build --release -vv


FROM debian:bookworm-slim AS runner
WORKDIR /app
COPY --from=builder /app/target/release/website-2 /app/
COPY --from=builder /app/target/site /app/site
COPY --from=builder /app/Cargo.toml /app/
ENV RUST_LOG="info"
ENV LEPTOS_SITE_ADDR="0.0.0.0:8080"
ENV LEPTOS_SITE_ROOT=./site
EXPOSE 8080
ENTRYPOINT ["/app/website-2"]
