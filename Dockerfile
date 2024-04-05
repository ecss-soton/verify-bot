FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef

WORKDIR /verify-bot

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /verify-bot/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --release --bin verify-bot

FROM debian:bookworm-slim AS runtime
WORKDIR /verify-bot

COPY --from=builder /verify-bot/target/release/verify-bot /usr/local/bin

# Install dependencies needed for verify-bot
RUN apt-get update && apt-get -y install libssl-dev openssl ca-certificates tzdata && apt upgrade -y openssl && apt clean && rm -rf /var/lib/apt/lists/*

ENTRYPOINT ["/usr/local/bin/verify-bot"]