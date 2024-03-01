FROM rust:1.76

WORKDIR /usr/src/verifybot
COPY . .

RUN cargo install --path .

CMD ["verifybot"]
