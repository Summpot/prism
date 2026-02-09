# Prism

Prism is a lightweight, high-performance TCP reverse proxy for the Minecraft protocol (L4). It routes incoming connections to different upstreams based on the hostname extracted from the first bytes of the stream (Minecraft handshake / TLS SNI / WASM).

- **Data plane**: TCP listener (commonly `:25565`)
- **Control plane**: HTTP admin server (default `:8080`)

For the intended architecture, see `DESIGN.md`.

## Getting started

Prism supports `.toml` and `.yaml`/`.yml` config files.

This repo also ships a JSON Schema for config validation and editor/LSP completion:

- `prism.schema.json`

- Run: `prism --config /path/to/prism.toml`
- Or set an env var: `PRISM_CONFIG=/path/to/prism.toml prism`
- Auto-discovery (from the current working directory): `prism.toml` > `prism.yaml` > `prism.yml`
- Fallback default path:
  - Linux: `/etc/prism/prism.toml`
  - Other OSes: `${ProjectConfigDir}/prism.toml` (derived from Rust's `directories::ProjectDirs`)

If the resolved config path does not exist, Prism will create a runnable default config file at that path and continue starting.

The auto-generated default config starts Prism in **tunnel server** mode (frp-like): it listens on `:7000/tcp` and waits for tunnel clients to connect and register services.

This repo includes example configs:

- `prism.example.toml`
- `prism.example.yaml`

### Config schema (validation + LSP)

#### YAML

If you use the YAML Language Server (for example in VS Code), you can add this to the top of your config:

```yaml
# yaml-language-server: $schema=./prism.schema.json
```

#### TOML

TOML doesn’t have a universal inline `$schema` directive. In VS Code, you can map the schema to your config filename pattern in settings:

```json
{
  "toml.schemas": {
    "./prism.schema.json": [
      "prism.toml",
      "**/prism.toml"
    ]
  }
}
```

### Run locally

1. Copy an example config into your working directory (for example, `prism.toml`)
2. Start Prism:

- Windows (PowerShell): `./prism.exe --config prism.toml`
- Linux/macOS: `./prism --config prism.toml`

### Routing rules

Use `routes` to map hostnames to upstream addresses.

`routes` is an **ordered list**: routes are checked in the order they appear, and the **first match wins**. Put more specific patterns earlier.

Each route specifies:

- `host` / `hosts`: one hostname pattern or a list of patterns
- `upstream` / `upstreams` (aliases: `backend` / `backends`): one or more upstream targets
- `strategy` (optional): how to pick an upstream when multiple are configured (`sequential`, `random`, `round-robin`)
- `cache_ping_ttl` (optional): Minecraft status (ping) response cache TTL (humantime like `60s`, `500ms`, `2m`; `-1` disables; default is a short TTL)

Hostname patterns support glob-like wildcards:

- `*` matches any string (captured as a group)
- `?` matches any single character (captured as a group)

If a pattern contains wildcards, any upstream strings may reference the captured groups as `$1`, `$2`, ...

Upstream targets are treated as TCP dial addresses. They can be IPs or DNS names.
If you omit the port (for example `backend.example.com`), Prism will prefer the
port from the Minecraft handshake when available; otherwise it falls back to the
port from the matched listener (default `25565`).

If multiple upstreams are configured, Prism will try them in the order produced by `strategy` and fall back to the next one if dialing fails.

### Routing parsers (WASM)

Prism extracts the routing hostname from the first bytes of each TCP connection using per-route parser chains.

By default, Prism enables two parsers implemented as **embedded WAT modules** (WebAssembly text format):

- `minecraft_handshake`
- `tls_sni`

Prism materializes these builtin parsers into `routing_parser_dir` at startup (if missing), then loads the `.wat` files referenced by your routes.

In config, each route can specify `parsers` as a string or list of strings (parser **names** only):

- `parsers = ["minecraft_handshake"]` -> loads `routing_parser_dir/minecraft_handshake.wat`
- `parsers = ["tls_sni"]` -> loads `routing_parser_dir/tls_sni.wat`

If `parsers` is omitted, Prism defaults to trying both builtin parsers in order: `["minecraft_handshake", "tls_sni"]`.

Prism intentionally **does not load raw `.wasm` binaries** for routing parsers.

## Tunnel mode

If your upstream server has **no public IP**, you can run Prism in a “tunnel client” role on the private machine and have it create an outbound tunnel to Prism running in the “server” role.

On the **public server**:

- Configure one or more proxy listeners (`listeners`) and `routes` as usual.
- Configure one or more tunnel endpoints under `tunnel.endpoints`.
  - Multiple listeners let you serve multiple transports at the same time (similar to frp's server).
- Point a route upstream at a tunnel service using `tunnel:<service>`.

On the **private machine**:

- Configure `tunnel.client` to connect to the public server.
- Configure the same `tunnel.auth_token` (if set).
- Register services under `tunnel.services`: `name -> local_addr`.
  - Optional: set `remote_addr` to request a server-side listener for the service (frp-like).
  - Optional: set `route_only=true` to ensure the service is **only** reachable via `tunnel:<service>` and never auto-exposed (must not set `remote_addr`).

If multiple tunnel clients register the same service `name`, Prism keeps the **first** active registrant as the routing target for `tunnel:<service>`. Later registrations with the same name do not override routing; they can still be exposed by port via `remote_addr` + `auto_listen_services`.

Transport notes:

- `tcp`: simplest, works everywhere.
- `udp`: reliable UDP (KCP) similar to frp's UDP-based mode.
- `quic`: QUIC streams over UDP (requires TLS; prisms can auto-generate a self-signed cert for convenience).

## Docker

This repository ships a `Dockerfile`, and the GitHub Actions workflow builds and pushes images to GHCR:

- `ghcr.io/<owner>/<repo>` (for example `ghcr.io/Summpot/prism`)

The container image uses `/config` as the working directory. If you mount a config file to `/config/prism.toml` (or `prism.yaml`/`prism.yml`), Prism will auto-discover it without extra flags.

### Run (Linux/macOS)

- Proxy: `25565/tcp`
- Admin: `8080/tcp`

```text
docker run --rm \
  -p 25565:25565 \
  -p 8080:8080 \
  -v "$PWD/prism.toml:/config/prism.toml:ro" \
  ghcr.io/Summpot/prism:latest
```

### Run (Windows PowerShell)

```text
docker run --rm `
  -p 25565:25565 `
  -p 8080:8080 `
  -v "${PWD}\prism.toml:/config/prism.toml:ro" `
  ghcr.io/Summpot/prism:latest
```

If your config file has a different name/path, pass it explicitly:

- `prism --config /config/myconfig.toml`

## Admin API

The admin server listens on `admin_addr` (default `:8080`).

- `GET /health` — health check (non-200 indicates unhealthy)
- `GET /metrics` — Prometheus text format metrics
- `GET /conns` — JSON active connection snapshot
- `GET /tunnel/services` — JSON snapshot of registered tunnel services
- `GET /config` — JSON with the resolved config file path
- `POST /reload` — trigger an on-demand config reload

## Build

You need Rust **stable** (MSRV: **1.88**) and Cargo.

- Build: `cargo build -p prism`
- Test: `cargo test --workspace`

Frontend (optional): this repo also contains a small admin UI.

- Tip: this repo uses Corepack; to match CI/Docker you can activate the latest pnpm via `corepack prepare pnpm@latest --activate`.

- Install: `pnpm install --frozen-lockfile`
- Build: `pnpm build`

## GitHub Actions

Workflow: `.github/workflows/build-release.yml`

- PR / push: runs `cargo test --workspace`
- PR / push: builds the frontend (`pnpm build`)
- PR / push: builds and uploads multi-platform `prism` binaries as workflow artifacts
- Tag (recommended format `v1.2.3`):
  - creates a GitHub Release with platform archives and `checksums.txt`
  - builds and pushes an Alpine-based multi-arch Docker image (`linux/amd64` + `linux/arm64`) to GHCR (native per-arch jobs, then manifest merge)
