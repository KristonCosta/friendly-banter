FROM rustlang/rust:nightly as builder
WORKDIR /usr/src/server
COPY server/ .
COPY common/ common/
RUN cargo install --path .

FROM debian:buster-slim
RUN apt-get update && apt-get install -y libssl-dev && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/server /usr/local/bin/server
CMD ["server"]
