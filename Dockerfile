# Multi-stage Dockerfile for Soltrace
# Stage 1: Builder - Compiles Rust binaries
FROM docker.io/library/rust:1.88-bookworm AS builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Copy source code
COPY . .

# Build dependencies (cached layer)
RUN cargo build --release
RUN ls target/ && pwd target/ && ls target/release/*

# Stage 2: Runtime - Contains only the binaries
FROM docker.io/library/rust:1.88-bookworm 

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN useradd -m -u 1000 soltrace && \
    mkdir -p /app /data /idls && \
    chown -R soltrace:soltrace /app /data /idls

# Set working directory
WORKDIR /app

# Copy binaries from builder
COPY --from=builder --chown=soltrace:soltrace \
    /app/target/release/soltrace-live \
    /app/target/release/soltrace-backfill \
    /app/

# Switch to non-root user
USER soltrace

# Create data directory for SQLite
VOLUME ["/data", "/idls"]

# Environment variables (with defaults)
ENV DB_URL=sqlite:./data/soltrace.db
ENV IDL_DIR=/idls
ENV SOLANA_RPC_URL=https://api.mainnet-beta.solana.com
ENV SOLANA_WS_URL=wss://api.mainnet-beta.solana.com
ENV COMMITMENT=confirmed
ENV RECONNECT_DELAY=5
ENV LIMIT=1000
ENV BATCH_SIZE=100
ENV BATCH_DELAY=100
ENV LOG_LEVEL=info

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD test -f /data/soltrace.db || exit 1

# Set entrypoint
ENTRYPOINT ["/app/soltrace-live"]

# Default command (can be overridden)
CMD ["run"]

# Metadata
LABEL maintainer="Fabian Schuh <fabian@chainsquad.com>"
LABEL description="Soltrace - IDL based Event Indexer for Solana"
LABEL version="0.1.0"
