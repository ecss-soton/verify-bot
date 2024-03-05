FROM rust:1.76 as builder

WORKDIR /usr/src/verify-vbot
COPY . .

RUN cargo install --path .

CMD ["verify-bot"]
