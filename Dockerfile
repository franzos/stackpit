FROM rust:1.88-slim AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src
COPY . .
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*
RUN groupadd -r stackpit && useradd -r -g stackpit stackpit
RUN mkdir -p /home/stackpit && chown stackpit:stackpit /home/stackpit
WORKDIR /home/stackpit
COPY --from=builder /build/target/release/stackpit /usr/local/bin/stackpit
USER stackpit
EXPOSE 3000 3001

# NOTE: Mount a volume at the configured storage.path (default: working dir)
# to persist the database. In docker-compose, map a named volume or host path
# to the container's WORKDIR, e.g.:
#   volumes:
#     - stackpit-data:/home/stackpit
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s \
  CMD curl -sf http://localhost:3001/health || exit 1

CMD ["stackpit", "serve"]
