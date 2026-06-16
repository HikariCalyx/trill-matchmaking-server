# Multi-stage build
FROM rust:1.81 as builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.toml
COPY build.rs build.rs
COPY proto proto
COPY src src

# Build application
RUN cargo build --release

# Final stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/tango-signaling-server /usr/local/bin/

EXPOSE 8000

ENV SERVER_HOST=0.0.0.0
ENV SERVER_PORT=8000
ENV RUST_LOG=info

CMD ["tango-signaling-server"]
