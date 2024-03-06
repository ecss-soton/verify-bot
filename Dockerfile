FROM rust:1.76-buster as build

RUN apt-get update && apt-get -y install libssl-dev openssl ca-certificates tzdata && apt upgrade -y openssl && apt clean && rm -rf /var/lib/apt/lists/*

# create a new empty shell project
RUN USER=root cargo new --bin verify-bot
WORKDIR /verify-bot

# copy over your manifests
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

# this build step will cache your dependencies
RUN cargo build --release
RUN rm src/*.rs

# copy your source tree
COPY ./src ./src
COPY ./log4rs.yml ./log4rs.yml

# build for release
RUN rm ./target/release/deps/verify_bot*
RUN cargo build --release

# our final base
FROM debian:buster

RUN apt-get update && apt-get -y install libssl-dev openssl ca-certificates tzdata && apt upgrade -y openssl && apt clean && rm -rf /var/lib/apt/lists/*

# copy the build artifact from the build stage
COPY --from=build /verify-bot/target/release/verify-bot .

# set the startup command to run your binary
CMD ["./verify-bot"]


