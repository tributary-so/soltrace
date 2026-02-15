# Docker for Soltrace

## Overview

Soltrace provides Docker support for easy deployment with multi-stage builds for optimal image size and caching.

## Quick Start

### 1. Clone Repository

```bash
git clone https://github.com/your-org/soltrace.git
cd soltrace
```

### 2. Configure Environment Variables

```bash
# Copy example environment file
cp .env.example .env

# Edit with your settings
nano .env
```

**Required Variables:**
- `PROGRAM_IDS`: Comma-separated list of program IDs to index

**Optional Variables:**
- `SOLANA_RPC_URL`: Solana RPC endpoint (default: mainnet)
- `SOLANA_WS_URL`: Solana WebSocket endpoint (default: mainnet)
- `DB_URL`: Database connection string (default: SQLite)
- `COMMITMENT`: Confirmation level (default: confirmed)

### 3. Build Docker Image

```bash
# Build with docker-compose
docker-compose build

# Or build directly
docker build -t soltrace:latest .
```

### 4. Run with Docker Compose

```bash
# Start live indexer
docker-compose up soltrace-live

# Start backfiller
docker-compose run --rm soltrace-backfill

# Start both
docker-compose up
```

## Docker Build Details

### Multi-Stage Build

The Dockerfile uses a **two-stage build**:

#### Stage 1: Builder
```dockerfile
FROM rust:1.75-bookworm AS builder

# Install dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev

# Copy and build
COPY . .
RUN cargo build --release
```

**Purpose:** Compiles Rust binaries with full toolchain

#### Stage 2: Runtime
```dockerfile
FROM debian:bookworm-slim

# Install runtime dependencies only
RUN apt-get install -y ca-certificates libssl3

# Copy binaries from builder
COPY --from=builder target/release/soltrace-* /app/
```

**Purpose:** Minimal runtime image with only binaries

**Benefits:**
- ✅ **Smaller image**: ~200MB vs ~2GB with full Rust toolchain
- ✅ **Faster builds**: Cargo cache in builder stage
- ✅ **Better security**: No build tools in runtime image
- ✅ **Layer caching**: Dependencies cached separately from source code

## Environment Variables

### Solana Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `SOLANA_RPC_URL` | `https://api.mainnet-beta.solana.com` | HTTP RPC endpoint |
| `SOLANA_WS_URL` | `wss://api.mainnet-beta.solana.com` | WebSocket endpoint |
| `CUSTOM_RPC_URL` | (none) | Override RPC URL |
| `CUSTOM_WS_URL` | (none) | Override WebSocket URL |

### Program Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `PROGRAM_IDS` | **Required** | Comma-separated program IDs |
| `IDL_DIR` | `/idls` | IDL files directory |

### Database Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `DB_URL` | `sqlite:./data/soltrace.db` | Database connection |
|  | `postgresql://user:pass@host/db` | PostgreSQL example |

### Live Indexer Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `COMMITMENT` | `confirmed` | Confirmation level |
| `RECONNECT_DELAY` | `5` | Reconnection delay (seconds) |

### Backfill Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `LIMIT` | `1000` | Signatures to fetch |
| `BATCH_SIZE` | `100` | Transactions per batch |
| `BATCH_DELAY` | `100` | Batch delay (ms) |

### Logging

| Variable | Default | Description |
|----------|---------|-------------|
| `LOG_LEVEL` | `info` | Logging verbosity |

## Docker Compose Services

### soltrace-live

Real-time event indexer with auto-reconnect.

```yaml
services:
  soltrace-live:
    restart: unless-stopped
    environment:
      - SOLANA_RPC_URL=${SOLANA_RPC_URL}
      - SOLANA_WS_URL=${SOLANA_WS_URL}
      - PROGRAM_IDS=${PROGRAM_IDS}
    volumes:
      - soltrace-data:/data
      - ./idls:/idls:ro
```

**Features:**
- Auto-restart on failure
- Persistent database volume
- Read-only IDL mount
- Health check
- Custom network

### soltrace-backfill

Historical event backfiller (runs once).

```yaml
services:
  soltrace-backfill:
    restart: "no"
    environment:
      - LIMIT=${LIMIT}
      - BATCH_SIZE=${BATCH_SIZE}
      - BATCH_DELAY=${BATCH_DELAY}
    depends_on:
      - soltrace-live
```

**Features:**
- Runs once, doesn't restart
- Shared database with live indexer
- Depends on live indexer
- Custom batch settings

## Volumes

### Named Volumes

```yaml
volumes:
  soltrace-data:
    driver: local
```

**Purpose:** Persistent SQLite database storage

**Location:** Docker managed volume (not on host)

### Host Volumes

To store database on host:

```yaml
volumes:
  - ./data:/data
  - ./idls:/idls:ro
```

**Structure:**
```
./
├── data/           # SQLite database
└── idls/           # IDL JSON files
    ├── program1.json
    └── program2.json
```

## Network Configuration

### Custom Network

```yaml
networks:
  soltrace-net:
    driver: bridge
```

**Benefits:**
- Isolated from other containers
- Custom subnet
- Service discovery by name

### Port Exposure

By default, ports are not exposed (internal network only).

To expose (not recommended for production):

```yaml
services:
  soltrace-live:
    ports:
      - "8080:8080"  # If you add metrics endpoint
```

## Health Check

```dockerfile
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD test -f /data/soltrace.db || exit 1
```

**Checks:**
- Database file exists
- Container is responsive
- Runs every 30 seconds

## Usage Examples

### Real-time Indexing

```bash
# Configure environment
echo "PROGRAM_IDS=TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA" > .env

# Start live indexer
docker-compose up -d soltrace-live

# View logs
docker-compose logs -f soltrace-live
```

### Historical Backfill

```bash
# Configure for backfill
echo "PROGRAM_IDS=TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA" >> .env
echo "LIMIT=1000" >> .env

# Run backfiller
docker-compose run --rm soltrace-backfill

# View progress
docker-compose logs -f soltrace-backfill
```

### Multiple Programs

```bash
# Index multiple programs
echo "PROGRAM_IDS=Prog1...,Prog2...,Prog3..." > .env

# Start indexer
docker-compose up -d soltrace-live
```

### Using PostgreSQL

```bash
# Configure PostgreSQL
echo "DB_URL=postgresql://soltrace:password@postgres:5432/soltrace" > .env

# Add PostgreSQL to docker-compose
# (see PostgreSQL section below)

# Start
docker-compose up -d
```

## Building Images

### Standard Build

```bash
docker build -t soltrace:latest .
```

### Build with BuildKit

```bash
DOCKER_BUILDKIT=1 docker build -t soltrace:latest .
```

### Build for Different Platforms

```bash
# Build for ARM (Raspberry Pi)
docker buildx build --platform linux/arm64 -t soltrace:latest .

# Build for multiple platforms
docker buildx build --platform linux/amd64,linux/arm64 -t soltrace:latest .
```

### Custom Rust Version

Edit `Dockerfile`:

```dockerfile
FROM rust:1.75-bookworm AS builder  # Change version
```

## Optimization Tips

### 1. Cargo Cache

The Dockerfile separates dependency installation from source code:

```dockerfile
# Layer 1: Cargo files (cached if unchanged)
COPY Cargo.toml Cargo.lock ./
RUN cargo build --release

# Layer 2: Source code
COPY . .
RUN cargo build --release
```

### 2. .dockerignore

Create `.dockerignore` to speed up builds:

```text
target/
.git/
.env
*.md
!README.md
```

### 3. BuildKit

Enable BuildKit for faster builds:

```bash
DOCKER_BUILDKIT=1 docker build .
```

### 4. Layer Caching

Multi-stage build ensures only changed layers are rebuilt:

```dockerfile
# Build dependencies (rarely changes)
COPY Cargo.toml Cargo.lock ./
RUN cargo build --release

# Build code (often changes)
COPY . .
RUN cargo build --release
```

## Production Deployment

### Using Docker Compose

```bash
# Production environment
cp .env.example .env
nano .env  # Configure for production

# Start
docker-compose up -d

# Check status
docker-compose ps

# View logs
docker-compose logs -f
```

### Using Docker Swarm

```yaml
version: '3.8'
services:
  soltrace-live:
    image: soltrace:latest
    deploy:
      replicas: 3
      update_config:
        parallelism: 1
        delay: 10s
      restart_policy:
        condition: on-failure
        delay: 5s
        max_attempts: 3
```

### Using Kubernetes

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: soltrace-live
spec:
  replicas: 3
  selector:
    matchLabels:
      app: soltrace-live
  template:
    metadata:
      labels:
        app: soltrace-live
    spec:
      containers:
      - name: soltrace-live
        image: soltrace:latest
        env:
        - name: PROGRAM_IDS
          valueFrom:
            configMapKeyRef:
              name: soltrace-config
              key: program-ids
        volumeMounts:
        - name: soltrace-data
          mountPath: /data
      volumes:
      - name: soltrace-data
        persistentVolumeClaim:
          claimName: soltrace-pvc
```

## Troubleshooting

### Container Won't Start

```bash
# Check logs
docker-compose logs soltrace-live

# Check environment variables
docker-compose config

# Verify database volume
docker volume inspect soltrace-data
```

### Database Errors

```bash
# Check database file
docker-compose exec soltrace-live ls -la /data/

# Reset database (if corrupted)
docker-compose down -v  # Remove volumes
docker-compose up -d
```

### Network Issues

```bash
# Check network
docker network inspect soltrace-net

# Test RPC connectivity
docker-compose exec soltrace-live curl -v https://api.mainnet-beta.solana.com
```

### Memory Issues

```bash
# Check memory usage
docker stats soltrace-live

# Increase memory limit
# In docker-compose.yml:
# services:
#   soltrace-live:
#     mem_limit: 2g
```

## Maintenance

### Update Container

```bash
# Rebuild image
docker-compose build

# Restart with new image
docker-compose up -d

# Remove old images
docker image prune -a
```

### Backup Database

```bash
# Copy database from container
docker cp soltrace-live:/data/soltrace.db ./backup_$(date +%Y%m%d).db

# Restore database
docker cp ./backup_20250209.db soltrace-live:/data/soltrace.db
docker-compose restart soltrace-live
```

### Clean Up

```bash
# Stop and remove containers
docker-compose down

# Remove volumes
docker-compose down -v

# Remove images
docker rmi soltrace:latest
```

## Security Considerations

### User Isolation

The container runs as non-root user:

```dockerfile
RUN useradd -m -u 1000 soltrace
USER soltrace
```

### Minimal Base Image

Uses `debian:bookworm-slim` (no build tools, minimal footprint).

### Read-Only Mounts

IDL directory is mounted read-only:

```yaml
volumes:
  - ./idls:/idls:ro  # ro = read-only
```

### No Sudo in Container

Container doesn't need or use sudo (non-root user).

## Performance Tuning

### Database Performance

For high-volume programs, consider PostgreSQL:

```yaml
services:
  postgres:
    image: postgres:15-alpine
    environment:
      POSTGRES_DB: soltrace
      POSTGRES_USER: soltrace
      POSTGRES_PASSWORD: password
    volumes:
      - postgres-data:/var/lib/postgresql/data

  soltrace-live:
    environment:
      - DB_URL=postgresql://soltrace:password@postgres:5432/soltrace
```

### Resource Limits

```yaml
services:
  soltrace-live:
    deploy:
      resources:
        limits:
          cpus: '2'
          memory: 2G
        reservations:
          cpus: '1'
          memory: 1G
```

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│ Docker Host                                           │
│                                                         │
│  ┌────────────────────────────────────────────────────┐   │
│  │ Docker Compose                                  │   │
│  │                                                 │   │
│  │  ┌──────────────┐  ┌─────────────────┐   │   │
│  │  │ soltrace-live  │  │ soltrace-backfill│   │   │
│  │  │               │  │                 │   │   │
│  │  │ - WebSocket    │  │ - RPC          │   │   │
│  │  │ - Real-time    │  │ - Historical     │   │   │
│  │  │ - Auto-reconnect│  │ - One-time      │   │   │
│  │  └──────────────┘  └─────────────────┘   │   │
│  │                                                 │   │
│  │  ┌──────────────────────────────────────┐   │   │
│  │  │ Volumes                                │   │   │
│  │  │ - soltrace-data (database)          │   │   │
│  │  │ - ./idls (IDL files, read-only)   │   │   │
│  │  └──────────────────────────────────────┘   │   │
│  │                                                 │   │
│  │  ┌──────────────────────────────────────┐   │   │
│  │  │ Network                                │   │   │
│  │  │ - soltrace-net (bridge)               │   │   │
│  │  └──────────────────────────────────────┘   │   │
│  └───────────────────────────────────────────────┘   │
│                                                         │
└──────────────────────────────────────────────────────────┘
```

## Files

| File | Purpose |
|-------|---------|
| `Dockerfile` | Multi-stage Docker image build |
| `docker-compose.yml` | Orchestration for multiple services |
| `.env.example` | Environment variable template |
| `DOCKER.md` | This documentation |

## Quick Reference

### Build
```bash
docker-compose build
```

### Run Live
```bash
docker-compose up -d soltrace-live
```

### Run Backfill
```bash
docker-compose run --rm soltrace-backfill
```

### Logs
```bash
docker-compose logs -f soltrace-live
```

### Stop
```bash
docker-compose down
```

## Support

- 📖 [Docker Documentation](https://docs.docker.com/)
- 📖 [Docker Compose Docs](https://docs.docker.com/compose/)
- 🐛 [Issues](https://github.com/your-org/soltrace/issues)
