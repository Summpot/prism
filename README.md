# Prism

Prism is a lightweight Rust reverse proxy with an frp-like tunnel mode.

Today, the implementation in this repository supports:

- **TCP listeners** in either hostname-routing mode or fixed-forward mode
- **UDP listeners** in fixed-forward mode
- **Per-route WAT middlewares** for parsing and rewriting connection preludes
- **Reverse tunnels** over TCP, reliable UDP (KCP), or QUIC
- **An HTTP admin API** for health, metrics, connection snapshots, reload signals, tunnel service snapshots, and managed control-plane APIs

> The frontend under `src/` is now a Prism management panel that targets the authenticated `/managed/*` APIs and can be deployed separately from the management node.

For implementation-level details, see [DESIGN.md](./DESIGN.md).

## What Prism does

Prism is aimed at Minecraft-style routing, where the hostname lives in the first bytes of the TCP stream, but the routing stack is more general than that. A TCP listener can capture the initial prelude, run one or more WebAssembly text modules (`.wat`) to extract a host, match that host against ordered routes, optionally rewrite the prelude for the selected upstream, and then proxy the rest of the session.

When a backend does not have a public IP, Prism can also run in tunnel mode. A tunnel client opens an outbound connection to a public Prism server, registers services, and lets the server reach those services either through `tunnel:<service>` routes or optional server-side auto-listened ports.

## Quick start

The repository already includes runnable example configs:

- `prism.example.toml`
- `prism.example.yaml`
- `prism.schema.json`

A minimal TCP hostname-routing config looks like this:

```toml
admin_addr = ":8080"

[[listeners]]
listen_addr = ":25565"
protocol = "tcp"

[[routes]]
host = "play.example.com"
upstream = "127.0.0.1:25565"
middlewares = ["minecraft_handshake"]
```

Then start Prism with either Cargo or a built binary:

```bash
cargo run -p prism -- --config ./prism.example.toml
```

or:

```bash
prism --config ./prism.example.toml
```

## Configuration and path resolution

Prism supports **TOML** and **YAML** (`.yaml` / `.yml`) config files.

### Config file lookup

Resolution order for the config file is:

1. `--config /path/to/prism.toml`
2. `PRISM_CONFIG=/path/to/prism.toml`
3. Auto-discovery in the current working directory:
   - `prism.toml`
   - `prism.yaml`
   - `prism.yml`
4. OS default path:
   - Linux: `/etc/prism/prism.toml`
   - Other OSes: the per-user config directory from `directories::ProjectDirs`

If the resolved config file does not exist, Prism creates a runnable default config and continues starting. That generated default config enables a **tunnel server** on `:7000` and an admin API on `:8080`.

### Runtime paths

Prism also resolves two runtime directories:

- **Workdir**
  - CLI: `--workdir`
  - Env: `PRISM_WORKDIR`
  - Default:
    - Linux: `/var/lib/prism`
    - Other OSes: the per-user data directory from `directories::ProjectDirs`

- **Middleware directory**
  - CLI: `--middleware-dir`
  - Env: `PRISM_MIDDLEWARE_DIR`
  - Default: `<config_dir>/middlewares`
  - Relative paths are resolved relative to the config directory

At startup, Prism materializes its built-in reference middlewares into the middleware directory **if the files do not already exist**.

## Configuration model

### Runtime roles and managed bootstrap

Prism now supports three top-level roles:

- `role = "standalone"` (default): traditional local-file Prism behavior
- `role = "management"`: runs the management control plane and persists managed node state in the workdir
- `role = "worker"`: runs a managed worker bootstrap that syncs desired config from a management node or exposes passive worker-agent endpoints

Managed deployments use the `managed` bootstrap section for identity and authentication:

- `managed.management.state_file`, `panel_token`, `worker_token`
- `managed.worker.node_id`, `management_url`, `auth_token`, `connection_mode`, `sync_interval_ms`, `agent_url`

In managed mode, the local file is bootstrap-focused. Desired runtime config is edited centrally through the management panel and synced as a structured config document.

### Listeners

`listeners` controls the public-facing proxy plane.

Each listener has:

- `listen_addr`
- `protocol = "tcp" | "udp"`
- optional `upstream`

Current behavior:

- **TCP + empty `upstream`**: hostname-routing mode
- **TCP + non-empty `upstream`**: fixed forwarding mode
- **UDP + non-empty `upstream`**: fixed forwarding mode

`":PORT"` shorthand is supported in config and normalized internally to `0.0.0.0:PORT`.

Important: **routes do not create listeners automatically**. If you want Prism to proxy traffic, you must configure one or more `listeners` explicitly.

### Routes

`routes` is an **ordered list** and **first match wins**.

Supported fields:

- `host` / `hosts`
- `upstream` / `upstreams`
- `backend` / `backends` (compatibility aliases)
- `middlewares`
- `parsers` (deprecated alias of `middlewares`)
- `strategy = "sequential" | "random" | "round-robin"`

Host patterns are matched case-insensitively and support:

- `*` → any string, captured as a wildcard group
- `?` → any single character, captured as a wildcard group

Wildcard captures can be reused in upstream templates as `$1`, `$2`, and so on.

If multiple upstreams are configured, Prism orders candidates using `strategy` and then dials them with failover until one succeeds.

Direct upstreams may omit the port. In that case Prism falls back to the listener port that accepted the connection.

### Middlewares

Route middlewares are **required** for hostname-routing routes.

Current middleware rules:

- Refer to modules by **name only**
- Names are normalized to lowercase
- `-` is normalized to `_`
- Paths and file extensions are rejected
- Prism loads `<middleware_dir>/<name>.wat`
- Raw `.wasm` binaries are intentionally **not** loaded

Built-in middlewares currently shipped by the repo:

- `minecraft_handshake`
- `tls_sni`

These modules support both:

- a **parse phase** to extract the routing host
- a **rewrite phase** to rewrite the captured prelude for the selected upstream

### Tunnel mode

Prism can run one or both tunnel roles in the same binary.

On the **public side**:

- configure `listeners` if you want public proxy ports
- configure `tunnel.endpoints` to accept tunnel clients
- route to a tunnel service using `tunnel:<service>`

On the **private side**:

- configure `tunnel.client`
- configure `tunnel.services`
- use the same `tunnel.auth_token` if the server requires one

Service fields:

- `name`
- `proto = "tcp" | "udp"`
- `local_addr`
- optional `remote_addr`
- optional `route_only = true`
- optional `masquerade_host`

Current semantics:

- `route_only = true` means the service can only be reached through `tunnel:<service>`
- `remote_addr` requests a server-side auto listener when `tunnel.auto_listen_services = true`
- `route_only = true` clears `remote_addr`
- if multiple tunnel clients register the same service name, the **first active registrant** remains the routing owner until it disconnects

Supported tunnel transports:

- `tcp` → TCP + yamux multiplexing
- `udp` → KCP over UDP + yamux multiplexing
- `quic` → QUIC streams over UDP

For QUIC endpoints, Prism can auto-generate a self-signed certificate when `cert_file` and `key_file` are both empty.

## Admin API

The admin server listens on `admin_addr`.

Implemented endpoints:

- `GET /health` → JSON `{ "ok": true }`
- `GET /metrics` → Prometheus text exposition
- `GET /conns` → JSON snapshot of active sessions
- `GET /tunnel/services` → JSON snapshot of registered tunnel services
- `GET /config` → JSON with the resolved config path
- `POST /reload` → sends a best-effort reload signal and returns a sequence number

Managed control-plane endpoints:

- `GET /managed/status`
- `GET /managed/nodes`
- `GET /managed/nodes/:node_id`
- `GET /managed/nodes/:node_id/config`
- `PUT /managed/nodes/:node_id/config`
- `POST /managed/worker/sync`
- `GET /managed/worker/status`
- `PUT /managed/worker/config`

Operational notes:

- The admin server only starts when `admin_addr` is non-empty **and** Prism has at least one enabled runtime role
- `/tunnel/services` returns `[]` when no tunnel manager is configured
- Legacy read endpoints remain available without built-in auth
- Managed endpoints use bearer auth (`panel_token` for panel access, `worker_token` / worker auth token for worker sync)
- The admin router still uses **permissive CORS**; authentication protects writes, but deployments should still scope exposure carefully

## Managed control plane

The management role persists a JSON control-plane state file under the Prism workdir and tracks:

- known nodes
- desired versus applied revisions
- restart-required state
- last-seen and last-apply status

Worker mode supports two bootstrap directions:

- **active**: the worker periodically syncs to `managed.worker.management_url` and receives desired config revisions
- **passive**: the worker exposes authenticated local agent endpoints so a reachable management node can inspect/apply config later

The current implementation ships the active sync loop end-to-end and exposes the passive worker-agent endpoints and state model, but does not yet include a full management-side passive polling/orchestration loop.

## Metrics, reloads, and logging

Current metrics emitted by the proxy path include:

- `prism_active_connections`
- `prism_connections_total`
- `prism_bytes_ingress_total`
- `prism_bytes_egress_total`
- `prism_route_hits_total{host="..."}`

Reload behavior today:

- file polling is controlled by `reload.enabled` and `reload.poll_interval_ms`
- `POST /reload` triggers the same reload path manually
- routes, middleware chains, and TCP runtime knobs are reloaded in place
- listener topology changes are **detected but not applied**; they require a restart
- logging configuration is initialized at startup and is **not** hot-reloaded today

Logging is configured under `logging` and supports:

- `level = debug | info | warn | error`
- `format = json | text`
- `output = stderr | stdout | discard | /path/to/file`
- `add_source = true | false`

## Docker

This repository ships a backend `Dockerfile`, and CI builds/publishes a container image to GHCR:

- `ghcr.io/<owner>/<repo>`

The image uses:

- config working directory: `/etc/prism`
- default Linux workdir: `/var/lib/prism`

Typical run command:

```bash
docker run --rm \
  -p 25565:25565 \
  -p 8080:8080 \
  -p 7000:7000 \
  -v "$PWD/prism.toml:/etc/prism/prism.toml:ro" \
  ghcr.io/Summpot/prism:latest
```

If you use UDP listeners or UDP-based tunnel transports, publish those ports as UDP explicitly. For example:

- UDP proxy listener: `-p 19132:19132/udp`
- QUIC or KCP tunnel endpoint: `-p 7001:7001/udp`

If you want Prism to auto-create config files or persist middleware/workdir state, mount directories instead of a single read-only config file.

## Frontend status

The repository root also contains a TanStack Start frontend under `src/`.

That frontend now ships a Prism panel experience with:

- connection setup for management base URL + panel bearer token
- dashboard and node inventory views
- node detail pages with desired/applied revision status
- a structured visual managed-config editor plus raw JSON preview

The panel is deployable separately and stores its connection settings locally in the browser.

## Build and test

Backend:

```bash
cargo build -p prism
cargo test --workspace
```

Prism is developed against **Rust stable**.

Frontend panel:

```bash
pnpm install --frozen-lockfile
pnpm build
pnpm test
pnpm check
```

## CI and release flow

Workflow: `.github/workflows/build-release.yml`

Current CI behavior:

- runs `cargo test --workspace`
- builds the frontend panel with `pnpm build`
- builds release binaries for multiple targets
- publishes backend Docker images to GHCR on non-PR builds
- publishes a separate frontend panel image as `ghcr.io/<owner>/<repo>-frontend`

## Schema and editor support

The repository ships `prism.schema.json` for config validation and completion.

### YAML

```yaml
# yaml-language-server: $schema=./prism.schema.json
```

### TOML (VS Code example)

```json
{
  "toml.schemas": {
    "./prism.schema.json": ["prism.toml", "**/prism.toml"]
  }
}
```
