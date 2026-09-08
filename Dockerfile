# Build stage - using musl for static linking
FROM rust:1.88-alpine AS builder

RUN apk add --no-cache musl-dev curl

WORKDIR /app

# Copy manifests first for better caching. The workspace members are
# path dependencies, so the workspace cannot resolve without them and the
# dependency-cache build below would do nothing at all.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Create dummy src to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release || true
RUN rm -rf src

# Copy actual source code
COPY src ./src

# Everything the crate reads at COMPILE time. `src/api/dev.rs` and
# `dev_auth.rs` pull the dev UI in with `include_str!`; `src/assets.rs`
# embeds the rest with `#[derive(RustEmbed)]`. A missing directory here is a
# compile error, not a missing file at runtime — and `fonts` and `screens`
# are needed in BOTH stages for that reason.
COPY static ./static
COPY screens ./screens
COPY fonts ./fonts
COPY docs/src ./docs/src
COPY byonk-base ./byonk-base
COPY default-config.yaml ./

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
# Ship the device-free default; config.yaml is the developer's local test config.
COPY default-config.yaml ./config.yaml

EXPOSE 3000

ENTRYPOINT ["/app/byonk"]
CMD ["serve"]
