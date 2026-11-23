# syntax=docker/dockerfile:1.7
#
# Multi-stage Dockerfile para rust-mcp-server.
#
# Stage 1 (`builder`): compila el fat-binary musl con cargo-zigbuild.
# Stage 2 (`runtime`): copia el binario a una imagen Alpine minimal.
#
# Build:    docker build -t rust-mcp-server:latest .
# Run:      docker run -i --rm rust-mcp-server:latest
# Test:     docker run -i --rm rust-mcp-server:latest < /dev/null
#           # (cierra stdin → server arranca y reporta "connection closed"
#           # que es el comportamiento esperado sin cliente MCP)

# ─── Stage 1: builder ──────────────────────────────────────────────
FROM rust:1.97-bookworm AS builder

# Sistema base: ya tiene cargo + rustc. Necesitamos añadir zig y
# cargo-zigbuild para el cross-compile a musl. zig NO está en los repos
# oficiales de Bookworm, así que descargamos el binario prebuilt.
ARG ZIG_VERSION=0.13.0
RUN apt-get update && apt-get install -y --no-install-recommends \
        xz-utils curl ca-certificates \
    && curl -fsSL "https://ziglang.org/download/${ZIG_VERSION}/zig-linux-x86_64-${ZIG_VERSION}.tar.xz" \
        -o /tmp/zig.tar.xz \
    && mkdir -p /opt/zig \
    && tar -xJf /tmp/zig.tar.xz -C /opt/zig --strip-components=1 \
    && rm /tmp/zig.tar.xz \
    && ln -sf /opt/zig/zig /usr/local/bin/zig

# Instala cargo-zigbuild (la versión que matchea con zig 0.13+).
# Lo instalamos en el user root porque en builds subsiguientes la cache
# de cargo lo reutiliza.
ARG CARGO_ZIGBUILD_VERSION=0.23.0
RUN cargo install cargo-zigbuild --locked --version ${CARGO_ZIGBUILD_VERSION}

WORKDIR /build

# Copiamos solo los manifests primero para cachear la compilación
# de dependencias (layer cache — si Cargo.toml no cambia, reusamos).
COPY Cargo.toml Cargo.lock version.txt ./
COPY src ./src
COPY tests ./tests
COPY scripts ./scripts
COPY build.rs ./

# El target por defecto es x86_64-unknown-linux-musl (la mayoría de
# servidores). Para aarch64, override: `--build-arg TARGET=aarch64-unknown-linux-musl`.
ARG TARGET=x86_64-unknown-linux-musl
RUN rustup target add ${TARGET}
# Override `codegen-units=1` (del Cargo.toml) a 16 para el build de musl.
# Con 1 el compilador usa demasiada RAM en `ra_ap_hir_ty` (~3 GB) y
# el container se mata con SIGKILL. Con 16 baja a ~700 MB y tarda un
# poco más. El binario final es idéntico.
RUN CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
    CARGO_PROFILE_RELEASE_LTO=false \
    cargo zigbuild --release --target ${TARGET} --locked

# ─── Stage 2: runtime ──────────────────────────────────────────────
# Alpine minimal — el fat-binary es estático, no necesita libc dinámico.
# Solo añadimos `git` (porque el server usa `git clone` para resolver
# Git URLs — ver §13 del proyecto).
FROM alpine:3.20 AS runtime
RUN apk add --no-cache git ca-certificates tini

# El binario compilado en el stage anterior. /build/target/<target>/release/.
ARG TARGET=x86_64-unknown-linux-musl
COPY --from=builder /build/target/${TARGET}/release/rust-mcp-server /usr/local/bin/rust-mcp-server

# Usuario no-root por seguridad (principio de least privilege).
RUN adduser -D -u 1000 mcp
USER mcp
WORKDIR /home/mcp

# Soporta dual transport:
# - HTTP por defecto (--mode http): expone el puerto 8080 en /mcp
# - stdio: podman|docker run -i --rm rust-mcp-server:latest --mode stdio
EXPOSE 8080
ENTRYPOINT ["/sbin/tini", "--", "/usr/local/bin/rust-mcp-server"]
CMD ["--mode", "http", "--port", "8080", "--host", "0.0.0.0"]

# Labels para documentar la imagen.
LABEL org.opencontainers.image.title="rust-mcp-server"
LABEL org.opencontainers.image.description="MCP server for exhaustive Rust code analysis (ground truth from ra_ap_*)"
LABEL org.opencontainers.image.source="https://github.com/softtek-ams/rust-mcp-server"
LABEL org.opencontainers.image.licenses="MIT"
