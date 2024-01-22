FROM rustlang/rust:nightly-slim AS builder

WORKDIR /build

COPY . .
RUN cargo build --release

FROM debian:12-slim

VOLUME /config

COPY --from=builder --chmod=755 /build/target/release/dpimyass /opt/dpimyass

CMD [ "/opt/dpimyass", "/config/config.toml" ]

RUN apt update -y && apt install xz-utils -y

ARG S6_OVERLAY_VERSION=3.1.6.2

ADD https://github.com/just-containers/s6-overlay/releases/download/v${S6_OVERLAY_VERSION}/s6-overlay-noarch.tar.xz /tmp
RUN tar -C / -Jxpf /tmp/s6-overlay-noarch.tar.xz

ADD https://github.com/just-containers/s6-overlay/releases/download/v${S6_OVERLAY_VERSION}/s6-overlay-x86_64.tar.xz /tmp
RUN tar -C / -Jxpf /tmp/s6-overlay-x86_64.tar.xz

ENTRYPOINT ["/init"]
