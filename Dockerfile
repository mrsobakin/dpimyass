FROM rust:1.79.0-slim AS builder
WORKDIR /build
COPY . .
RUN rustup target add x86_64-unknown-linux-musl
RUN cargo build --release --target=x86_64-unknown-linux-musl

FROM alpine:3.19
VOLUME /config
COPY --from=builder --chmod=755 /build/target/x86_64-unknown-linux-musl/release/dpimyass /opt/dpimyass
CMD [ "/opt/dpimyass", "/config/config.toml" ]
