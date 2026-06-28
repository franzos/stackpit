# syntax=docker/dockerfile:1

# Build stage. One Dockerfile, two backends selected via DB_FEATURE
# (sqlite|postgres) — Stackpit's db features are mutually exclusive, so each
# image bundles exactly one. sqlx is pure-Rust over rustls (no libpq/OpenSSL);
# the sqlite backend statically compiles bundled libsqlite3, which needs a C
# toolchain — already present in the rust image.
FROM rust:1-slim-trixie AS builder
ARG DB_FEATURE=sqlite
WORKDIR /app

# Cache dependencies: copy the workspace manifests (root + members), build a
# stub, then build for real once the sources land.
COPY Cargo.toml Cargo.lock ./
COPY stackpit-auth/Cargo.toml stackpit-auth/
RUN mkdir src stackpit-auth/src \
    && echo "fn main() {}" > src/main.rs \
    && touch src/lib.rs stackpit-auth/src/lib.rs \
    && cargo build --release --no-default-features --features "$DB_FEATURE" \
    && rm -rf src stackpit-auth/src
COPY . .
RUN touch src/main.rs stackpit-auth/src/lib.rs \
    && cargo build --release --no-default-features --features "$DB_FEATURE"

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/stackpit /app/stackpit
EXPOSE 3000 3001

# NOTE: Mount a volume at the configured storage.path (default: working dir)
# to persist the database. In docker-compose, map a named volume or host path
# to the container's WORKDIR, e.g.:
#   volumes:
#     - stackpit-data:/app
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s \
  CMD curl -sf http://localhost:3001/health || exit 1

CMD ["./stackpit", "serve"]
