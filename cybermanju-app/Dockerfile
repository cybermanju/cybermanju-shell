# ═══════════════════════════════════════════════════════════════════════
# CyberManju OS — Multi-stage Docker Build
#
# Stage 1: Build Vue 3 frontend (Node.js)
# Stage 2: Build standalone Rust web server (no Tauri/GTK deps)
# Stage 3: Minimal Alpine runtime
# ═══════════════════════════════════════════════════════════════════════

# ─── Stage 1: Frontend Build ──────────────────────────────────────────
FROM node:24-alpine AS frontend-builder

WORKDIR /app

# Install dependencies first (layer caching). The `--mount=type=cache` keeps
# the npm download cache across builds (it is NOT part of the image layers),
# and `npm ci` (not `npm install`) makes the layer byte-deterministic.
COPY package.json package-lock.json* ./
RUN --mount=type=cache,target=/root/.npm npm ci

# Copy frontend source and build for web deployment (no Tauri)
COPY index.html ./
COPY tsconfig.json tsconfig.node.json env.d.ts ./
COPY vite.config.wasm.ts vite.config.ts ./
COPY vite-plugin-wasm-stub.ts wasm-pkg.ts ./
COPY public/ ./public/
COPY keymaps/ ./keymaps/
COPY src/ ./src/
# Scene tables: src/utils/scene.ts imports ../../crates/scene/data/scenes.json
# (same file the Rust classifier uses) — it must exist in this stage.
COPY crates/scene/data/ ./crates/scene/data/
# prebuild:wasm:frontend runs `npm run icons` → scripts/generate-icon-set.mjs
COPY scripts/ ./scripts/

# The static bundle is served from root (base: "/") both by the GitHub
# Pages site (cybermanju.github.io) and by this image. The docker image
# serves the
# dashboard REST API on :3456, so the wasm backend is not needed here — the
# frontend-only vite step uses the stub plugin (no wasm-pack in this stage).
#
# OAuth broker (Supabase project URL + public anon key): baked into the
# served bundle at build time so sign-in works out of the box. Both values
# are public — pass with --build-arg when available. Empty keeps the old
# behaviour: the app then takes the Settings → OAuth broker paste.
ARG VITE_SUPABASE_URL=""
ARG VITE_SUPABASE_ANON_KEY=""
ENV VITE_SUPABASE_URL=$VITE_SUPABASE_URL VITE_SUPABASE_ANON_KEY=$VITE_SUPABASE_ANON_KEY
RUN DOCKER_BUILD=true npm run build:wasm:frontend

# ─── Stage 2: Rust Backend Build ─────────────────────────────────────
FROM rust:alpine AS backend-builder

# build-base (gcc/g++/make/musl-dev) + cmake/perl are required to compile the
# C/asm parts of `ring` (pulled in by rustls → reqwest → cybermanju-sync)
RUN apk add --no-cache build-base pkgconf cmake perl

# pqcrypto-mlkem's vendored PQClean `compat.h` gates a polyfill on
# `__GNUC_PREREQ(7,1)`, which only glibc's <features.h> defines — on musl the
# preprocessor sees `!__GNUC_PREREQ(7,1)` with the macro undefined and stops
# with "missing binary operator before token '('". Defining it as true keeps
# the modern-GCC path (the polyfill is only for GCC < 7.1, and this image
# ships a current one).
ENV CFLAGS="-D__GNUC_PREREQ(major,minor)=1"

WORKDIR /build

# ──1. Workspace manifests (change rarely → keeps the dependency layer) ──
COPY Cargo.toml Cargo.lock ./
COPY crates/types/Cargo.toml          crates/types/Cargo.toml
COPY crates/crypto/Cargo.toml         crates/crypto/Cargo.toml
COPY crates/compression/Cargo.toml    crates/compression/Cargo.toml
COPY crates/search/Cargo.toml         crates/search/Cargo.toml
COPY crates/db/Cargo.toml             crates/db/Cargo.toml
COPY crates/web/Cargo.toml            crates/web/Cargo.toml
COPY crates/sync/Cargo.toml           crates/sync/Cargo.toml
COPY crates/faces/Cargo.toml          crates/faces/Cargo.toml
COPY crates/tests/Cargo.toml          crates/tests/Cargo.toml
COPY crates/scene/Cargo.toml          crates/scene/Cargo.toml
COPY crates/os/Cargo.toml             crates/os/Cargo.toml
COPY crates/disk/Cargo.toml           crates/disk/Cargo.toml
COPY crates/erasure/Cargo.toml        crates/erasure/Cargo.toml
COPY crates/agent/Cargo.toml          crates/agent/Cargo.toml
COPY crates/os-wasm/Cargo.toml        crates/os-wasm/Cargo.toml
COPY src-tauri/Cargo.toml             src-tauri/Cargo.toml
COPY docker/server/Cargo.toml         docker/server/Cargo.toml

# ──2. Stub sources: compile every third-party dependency once ───────────
#     (the real sources land in the next COPY and only rebuild our crates)
RUN mkdir -p crates/*/src src-tauri/src docker/server/src && \
    for d in crates/*/; do printf '// stub\n' > "$d/src/lib.rs"; done && \
    printf '// stub\n' > src-tauri/src/lib.rs && \
    printf 'fn main() {}\n' > src-tauri/src/main.rs && \
    printf 'fn main() {}\n' > docker/server/src/main.rs && \
    cargo build --release -p cybermanju-os-server || true

# ──3. Real sources ────────────────────────────────────────────────────
COPY crates ./crates
COPY src-tauri ./src-tauri
COPY docker/server ./docker/server

RUN cargo build --release -p cybermanju-os-server

# ─── Stage 3: Minimal Runtime ────────────────────────────────────────
FROM alpine:3.21 AS runtime

# ca-certificates for HTTPS, wget for healthcheck
RUN apk add --no-cache ca-certificates wget

# Create non-root user for security
RUN addgroup -S cybermanju && adduser -S cybermanju -G cybermanju

WORKDIR /app

# Copy the compiled Rust binary from stage 2
COPY --from=backend-builder /build/target/release/cybermanju-os-server ./

# Copy the compiled Vue frontend from stage 1
COPY --from=frontend-builder /app/dist-wasm ./static

# Create data directory for database and config
RUN mkdir -p /data && chown -R cybermanju:cybermanju /app /data

# Switch to non-root user
USER cybermanju

# Expose the web dashboard port
EXPOSE 3456

# Environment variables (can be overridden in docker-compose.yml)
ENV RUST_LOG=info
ENV PORT=3456
ENV DB_PATH=/data/cybermanju.db
ENV SEARCH_INDEX_PATH=/data/tantivy_index
ENV STATIC_DIR=/app/static
ENV TZ=UTC

# Volume mount point for persistent data
VOLUME ["/data"]

# Health check hits the readiness endpoint: 200 only when redb, the Tantivy
# index and the data volume are all usable (/api/health is liveness).
# Fetch with GET: wget --spider sends HEAD, which this route does not serve.
HEALTHCHECK --interval=30s --timeout=10s --retries=3 --start-period=15s \
    CMD wget -q -O /dev/null http://localhost:3456/api/readyz || exit 1

# Start the server
CMD ["./cybermanju-os-server"]
