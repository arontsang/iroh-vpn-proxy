FROM --platform=$BUILDPLATFORM rust:alpine AS builder

RUN apk add --no-cache py3-pip && pip install ziglang --break-system-packages && cargo install cargo-zigbuild
FROM builder AS dependencies-compiler
ARG TARGETARCH=amd64

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/hello.rs ./src/hello.rs

RUN case "$TARGETARCH" in \
      "amd64")  echo "x86_64-unknown-linux-musl" > /tmp/target ;; \
      "arm64")  echo "aarch64-unknown-linux-musl" > /tmp/target ;; \
      *)              echo "x86_64-unknown-linux-musl" > /tmp/target ;; \
    esac && \
    rustup target add "$(cat /tmp/target)"
RUN --mount=type=cache,target=/root/.cargo/registry,id=registry-$TARGETARCH \
    --mount=type=cache,target=/app/target,id=compile-$TARGETARCH \
    RUST_TARGET=$(cat /tmp/target) && \
    cargo zigbuild --release --target "$RUST_TARGET" --bin hello

FROM dependencies-compiler AS compiler

COPY src/ src/

RUN --mount=type=cache,target=/root/.cargo/registry,id=registry-$TARGETARCH \
    --mount=type=cache,target=/app/target,id=compile-$TARGETARCH \
    RUST_TARGET=$(cat /tmp/target) && \
    touch src/$TARGET_BIN.rs &&\
    cargo zigbuild --release --target "$RUST_TARGET"
RUN --mount=type=cache,target=/root/.cargo/registry,id=registry-$TARGETARCH \
    --mount=type=cache,target=/app/target,id=compile-$TARGETARCH \
    RUST_TARGET=$(cat /tmp/target) && \
    mkdir /dist && \
    cp /app/target/$RUST_TARGET/release/client /app/target/$RUST_TARGET/release/server /dist

FROM alpine:latest
ARG TARGET_BIN=server

COPY --from=compiler /dist/$TARGET_BIN /opt/app/bin

ENV HTTP_PORT=80
ENV QUIC_PORT=0
EXPOSE 80
WORKDIR /opt/app

ENTRYPOINT ["./bin"]
