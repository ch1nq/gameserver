FROM lukemathwalker/cargo-chef:0.1.68-rust-alpine AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder 
RUN apk update && \
    apk add --no-cache bash curl npm libc-dev binaryen
RUN npm install -g sass
RUN curl --proto '=https' --tlsv1.2 -LsSf https://github.com/leptos-rs/cargo-leptos/releases/latest/download/cargo-leptos-installer.sh | sh
RUN rustup target add wasm32-unknown-unknown
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --target x86_64-unknown-linux-musl --recipe-path recipe.json
COPY . .
RUN cargo leptos build --release -vv --target x86_64-unknown-linux-musl
# RUN cargo build --release --bin app

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
