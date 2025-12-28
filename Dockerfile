# Build stage - using musl for static linking
FROM rust:1.85-alpine AS builder

RUN apk add --no-cache musl-dev

WORKDIR /app

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock ./

# Create dummy src to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release || true
RUN rm -rf src

# Copy actual source code
COPY src ./src

# Build the release binary (statically linked)
RUN cargo build --release

# Runtime stage - minimal image with just the binary
FROM scratch

# Copy CA certificates for HTTPS
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/byonk /app/byonk

# Copy static assets
COPY fonts ./fonts
COPY screens ./screens
COPY config.yaml ./config.yaml

EXPOSE 3000

ENTRYPOINT ["/app/byonk"]
