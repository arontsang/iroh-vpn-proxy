FROM --platform=$BUILDPLATFORM rust:alpine AS builder

RUN apk add --no-cache py3-pip && pip install ziglang --break-system-packages && cargo install cargo-zigbuild

FROM builder AS compiler

ARG TARGETPLATFORM
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/app/target \
    case "$TARGETPLATFORM" in \
      "linux/amd64")  RUST_TARGET="x86_64-unknown-linux-musl" ;; \
      "linux/arm64")  RUST_TARGET="aarch64-unknown-linux-musl" ;; \
      *)              RUST_TARGET="x86_64-unknown-linux-musl" ;; \
    esac && \
    rustup target add "$RUST_TARGET" && \
    cargo zigbuild --release --target "$RUST_TARGET" --bin server
RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/app/target \
    case "$TARGETPLATFORM" in \
      "linux/amd64")  RUST_TARGET="x86_64-unknown-linux-musl" ;; \
      "linux/arm64")  RUST_TARGET="aarch64-unknown-linux-musl" ;; \
      *)              RUST_TARGET="x86_64-unknown-linux-musl" ;; \
    esac && \
    mkdir /dist && \
    cp "target/$RUST_TARGET/release/server" /dist/server

ARG TARGETPLATFORM
FROM --platform=$TARGETPLATFORM alpine:latest

COPY --from=compiler /dist/server /usr/local/bin/server

ENV HTTP_PORT=80
ENV QUIC_PORT=0
EXPOSE 80


ENTRYPOINT ["server"]