FROM rust:1.85-alpine AS builder

RUN apk add --no-cache musl-dev gcc make perl

WORKDIR /build

COPY Cargo.toml ./
RUN mkdir -p src && echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src target/release/deps/pyload_proxy_for_sonarr* \
                  target/release/pyload-proxy-for-sonarr*

COPY src ./src
RUN cargo build --release && \
    strip target/release/pyload-proxy-for-sonarr

FROM scratch

COPY --from=builder /build/target/release/pyload-proxy-for-sonarr /pyload-proxy-for-sonarr

USER 1000:1000
EXPOSE 8080
ENV PORT=8080 RUST_LOG=info

HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
  CMD ["/pyload-proxy-for-sonarr", "--healthcheck"]

ENTRYPOINT ["/pyload-proxy-for-sonarr"]
