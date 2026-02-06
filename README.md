# Prism

Prism is a lightweight, high-performance TCP reverse proxy for the Minecraft protocol (L4). It routes incoming connections to different upstreams based on the hostname extracted from the first bytes of the stream (Minecraft handshake / TLS SNI / WASM).

- **Data plane**: TCP listener (default `:25565`)
- **Control plane**: HTTP admin server (default `:8080`)

For the intended architecture, see `DESIGN.md`.

## Getting started

Prism supports `.toml`, `.yaml`/`.yml`, and `.json` config files.

- Explicit: `prism -config /path/to/prism.json`
- Auto-discovery (from the current working directory): `prism.toml` > `prism.yaml` > `prism.yml` > `prism.json`

This repo includes example configs:

- `config.example.json`
- `prism.example.toml`
- `prism.example.yaml`

### Run locally

1. Copy an example config into your working directory (for example, `prism.json`)
2. Start Prism:

- Windows (PowerShell): `./prism.exe -config prism.json`
- Linux/macOS: `./prism -config prism.json`

### Routing rules

Use `routes` to map hostnames to upstream addresses:

- Exact match: `play.example.com` → `127.0.0.1:25566`
- Wildcard suffix: `*.labs.example.com` → `127.0.0.1:25567`

Upstream targets are treated as TCP dial addresses. They can be IPs or DNS names.
If you omit the port (for example `backend.example.com`), Prism will prefer the
port from the Minecraft handshake when available; otherwise it falls back to the
port from `listen_addr` (default `25565`).

Wildcard routes are `*.`-prefixed suffix matches (and more specific suffixes win).

## Docker

This repository ships a `Dockerfile`, and the GitHub Actions workflow builds and pushes images to GHCR:

- `ghcr.io/<owner>/<repo>` (for example `ghcr.io/Summpot/prism`)

The container image uses `/config` as the working directory. If you mount a config file to `/config/prism.json` (or `prism.toml`/`prism.yaml`/`prism.yml`), Prism will auto-discover it without extra flags.

### Run (Linux/macOS)

- Proxy: `25565/tcp`
- Admin: `8080/tcp`

```text
docker run --rm \
  -p 25565:25565 \
  -p 8080:8080 \
  -v "$PWD/prism.json:/config/prism.json:ro" \
  ghcr.io/Summpot/prism:latest
```

### Run (Windows PowerShell)

```text
docker run --rm `
  -p 25565:25565 `
  -p 8080:8080 `
  -v "${PWD}\prism.json:/config/prism.json:ro" `
  ghcr.io/Summpot/prism:latest
```

If your config file has a different name/path, pass it explicitly:

- `prism -config /config/myconfig.toml`

## Admin API

The admin server listens on `admin_addr` (default `:8080`).

- `GET /health` — health check (non-200 indicates unhealthy)
- `GET /metrics` — JSON metrics snapshot
- `GET /conns` — JSON active connection snapshot
- `GET /logs?limit=200` — recent log lines (requires `logging.admin_buffer.enabled=true`)
- `POST /reload` — trigger an on-demand config reload (requires reload to be enabled)

## Build

You need Go (version is defined in `go.mod`).

- Build: `go build ./cmd/prism`
- Test: `go test ./...`

## GitHub Actions

Workflow: `.github/workflows/build-release.yml`

- PR / push: runs `go test ./...` and uploads multi-platform binaries as artifacts
- Tag (recommended format `v1.2.3`):
  - creates a GitHub Release with platform archives and `checksums.txt`
  - builds and pushes an Alpine-based multi-arch Docker image (`linux/amd64` + `linux/arm64`) to GHCR (native per-arch jobs, then manifest merge)
