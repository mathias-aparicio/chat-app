FROM rust:slim as builder

WORKDIR /usr/src/app

# Needed for compilling rdkafka
RUN apt-get update && apt-get install -y \
  pkg-config \
  libssl-dev \
  build-essential \
  cmake \
  && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release

COPY . .
RUN touch src/main.rs
RUN cargo build --release

FROM debian:stable-slim

WORKDIR /app


COPY --from=builder /usr/src/app/target/release/chat-app ./chat-app

COPY templates ./templates
COPY .cql ./.cql 

EXPOSE 8000

CMD ["./chat-app"]
