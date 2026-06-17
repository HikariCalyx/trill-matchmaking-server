# Multi-stage build
FROM rust:1.81 as builder

WORKDIR /app

# Copy manifests (Cargo.lock for reproducible builds)
COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock
COPY src src

# Build application
RUN cargo build --release

# Final stage
FROM debian:bookworm-slim

# No system OpenSSL/ca-certificates needed: the binary uses rustls with
# bundled Mozilla root certificates (webpki-roots), statically compiled.
COPY --from=builder /app/target/release/trill-signaling-server /usr/local/bin/

EXPOSE 8000

ENV SERVER_HOST=0.0.0.0
ENV SERVER_PORT=8000
ENV RUST_LOG=info

CMD ["trill-signaling-server"]
