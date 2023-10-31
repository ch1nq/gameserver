FROM rust:1.73 as builder
WORKDIR /usr/src/gameserver
COPY ./server/Cargo.toml ./server/Cargo.lock .
COPY ./server/src ./src
RUN cargo install --path .

FROM debian:bookworm-slim
RUN apt-get update && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/gameserver ./gameserver
COPY ./server/www ./www
CMD ["./gameserver", "--game", "achtung", "--num-players", "8"]
