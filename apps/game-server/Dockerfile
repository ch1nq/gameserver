FROM rust:1.81 as builder
WORKDIR /usr/src/game-server
COPY /Cargo.toml .
COPY /Cargo.lock .
COPY /src ./src
RUN cargo install --path .

FROM debian:bookworm-slim
RUN apt-get update && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/game-server ./game-server

CMD ["./game-server"]

