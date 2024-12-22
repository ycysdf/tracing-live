FROM rustlang/rust:nightly-slim AS builder
WORKDIR /myapp
RUN apt update && apt install -y musl-tools musl-dev libprotobuf-dev protobuf-compiler  && rm -rf /var/lib/apt/lists/* && rustup target add x86_64-unknown-linux-musl
COPY . .
RUN cargo build --release --bin tracing-lv-server --target x86_64-unknown-linux-musl

FROM alpine:3
ENV DATABASE_URL="postgresql://postgres:123456@host.docker.internal:5432/postgres"
ENV AUTO_INIT_DATABASE="true"
EXPOSE 443
EXPOSE 8080
COPY --from=builder /myapp/target/x86_64-unknown-linux-musl/release/tracing-lv-server /usr/local/bin/tracing-lv-server
ENTRYPOINT ["tracing-lv-server"]

