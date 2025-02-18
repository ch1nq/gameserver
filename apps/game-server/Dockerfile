FROM rust:1.81 as builder
WORKDIR /usr/src/game-server
COPY apps/game-server/Cargo.toml .
COPY apps/game-server/Cargo.lock .
COPY libs/arcadio /usr/libs/arcadio
COPY apps/game-server/src ./src
RUN cargo install --path .

FROM debian:bookworm-slim
RUN apt-get update && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/game-server ./game-server

CMD ["./game-server"]

