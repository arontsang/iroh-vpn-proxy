FROM --platform=$BUILDPLATFORM rust:alpine AS builder

RUN apk add --no-cache py3-pip && pip install ziglang --break-system-packages && cargo install cargo-zigbuild

FROM builder AS dependencies-compiler

ARG TARGETPLATFORM
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/hello.rs ./src/hello.rs

ARG CARGO_TARGET_DIR=/tmp/build/target
ENV CARGO_TARGET_DIR=$CARGO_TARGET_DIR
RUN case "$TARGETPLATFORM" in \
      "linux/amd64")  echo "x86_64-unknown-linux-musl" > /tmp/target ;; \
      "linux/arm64")  echo "aarch64-unknown-linux-musl" > /tmp/target ;; \
      *)              echo "x86_64-unknown-linux-musl" > /tmp/target ;; \
    esac && \
    rustup target add "$(cat /tmp/target)"
RUN RUST_TARGET=$(cat /tmp/target) && \
    cargo zigbuild --release --target "$RUST_TARGET" --bin hello --target-dir $CARGO_TARGET_DIR

FROM dependencies-compiler AS compiler

ARG TARGET_BIN=server
COPY src/ src/

RUN RUST_TARGET=$(cat /tmp/target) && \
    touch src/$TARGET_BIN.rs &&\
    cargo zigbuild --release --target "$RUST_TARGET" --bin $TARGET_BIN --target-dir $CARGO_TARGET_DIR
RUN RUST_TARGET=$(cat /tmp/target) && \
    mkdir /dist && \
    cp "$CARGO_TARGET_DIR/$RUST_TARGET/release/$TARGET_BIN" /dist/$TARGET_BIN

ARG TARGETPLATFORM
FROM --platform=$TARGETPLATFORM alpine:latest
ARG TARGET_BIN=server

COPY --from=compiler /dist/$TARGET_BIN /opt/app/bin

ENV HTTP_PORT=80
ENV QUIC_PORT=0
EXPOSE 80
WORKDIR /opt/app

ENTRYPOINT ["./bin"]