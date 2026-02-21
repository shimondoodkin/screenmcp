FROM rust:1.85-slim-bookworm

WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

CMD ["cargo", "run", "--release", "--features", "api"]
