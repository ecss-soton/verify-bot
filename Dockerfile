FROM rust:1.67

WORKDIR /usr/src/verifybot
COPY . .

RUN cargo install --path .

CMD ["verifybot"]
