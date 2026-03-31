# Prism Design

This document describes the **current Rust implementation** in this repository.

It is intentionally implementation-grounded. If the code changes in a way that affects public behavior, this document should change with it.

## 1. Scope

Prism is a single-binary reverse proxy with an frp-like tunnel mode.

The implementation currently supports:

- TCP listeners in **hostname-routing** mode or **fixed-forward** mode
- UDP listeners in **fixed-forward** mode
- Per-route WebAssembly text middlewares (`.wat`) for parse and rewrite behavior
- Reverse tunnels over **TCP**, **KCP-over-UDP**, and **QUIC**
- An HTTP admin API for health, metrics, connection snapshots, reload signals, tunnel service snapshots, and managed control-plane APIs

The repository also contains a TanStack frontend under `src/`, and that frontend now acts as a Prism management panel for the authenticated `/managed/*` API surface.

## 2. Repository map

- Runtime entrypoint: `crates/prism/src/main.rs`
- Runtime orchestration: `crates/prism/src/prism/app.rs`
- Configuration loading: `crates/prism/src/prism/config.rs`
- Runtime path resolution: `crates/prism/src/prism/runtime_paths.rs`
- Routing: `crates/prism/src/prism/router.rs`
- Middleware runtime: `crates/prism/src/prism/middleware.rs`
- Proxy plane: `crates/prism/src/prism/proxy.rs`
- Tunnel mode: `crates/prism/src/prism/tunnel/*`
- Admin API: `crates/prism/src/prism/admin.rs`
- Telemetry: `crates/prism/src/prism/telemetry.rs`
- Logging: `crates/prism/src/prism/logging.rs`

## 3. Startup and role enablement

### 3.1 CLI inputs

The Prism binary accepts three top-level runtime path inputs:

- `--config` / `PRISM_CONFIG`
- `--workdir` / `PRISM_WORKDIR`
- `--middleware-dir` / `PRISM_MIDDLEWARE_DIR`

### 3.2 Config path resolution

Config lookup order is:

1. explicit `--config`
2. `PRISM_CONFIG`
3. current-working-directory auto-discovery:
   - `prism.toml`
   - `prism.yaml`
   - `prism.yml`
4. OS default path:
   - Linux: `/etc/prism/prism.toml`
   - Other OSes: the per-user config dir from `directories::ProjectDirs`

If the resolved config file does not exist, Prism creates a runnable default config and keeps starting. The generated default config enables a tunnel server on `:7000` and an admin server on `:8080`.

### 3.3 Runtime paths

Prism resolves two additional runtime paths:

- **workdir**
  - Linux default: `/var/lib/prism`
  - other OSes: per-user data dir from `directories::ProjectDirs`
- **middleware_dir**
  - default: `<config_dir>/middlewares`
  - relative values are resolved relative to the config directory

At startup, Prism materializes built-in middleware files into `middleware_dir` if they do not already exist.

### 3.4 Runtime roles

Prism supports three explicit runtime roles:

- `role = standalone` (default)
- `role = management`
- `role = worker`

Role-specific enablement rules:

- **Standalone** preserves the traditional behavior: proxy/tunnel roles are inferred from `listeners`, `tunnel.endpoints`, and `tunnel.client + tunnel.services`
- **Management** may run with **no proxy or tunnel workload** as long as `admin_addr` is set and `managed.management` is configured
- **Worker** may run with **no local traffic workload** as long as `admin_addr` is set and `managed.worker` is configured
- **Admin role** is enabled when `admin_addr` is non-empty and either a traditional runtime role is enabled or `role` is `management` / `worker`

Important current behavior: **routes alone do not create listeners**. If you want Prism to accept public proxy traffic, you must configure `listeners` explicitly.

If no runtime role is enabled, Prism exits with an error instead of running a partial process.

## 4. Configuration model

### 4.1 Supported file formats

Prism supports:

- TOML
- YAML (`.yaml`, `.yml`)

JSON is not supported as a runtime config format.

### 4.2 Role and managed bootstrap

New top-level config fields:

- `role = standalone | management | worker`
- `managed.management`
- `managed.worker`

`managed.management` fields:

- `state_file`
- `panel_token`
- `worker_token`

`managed.worker` fields:

- `node_id`
- `management_url`
- `auth_token`
- `connection_mode = active | passive`
- `sync_interval_ms`
- `agent_url`

Current managed semantics:

- the **management** role is the only writable source of truth for managed runtime config
- a **worker** keeps bootstrap identity and auth in the local file, but desired runtime config is tracked separately as structured managed state
- active workers dial the management node and exchange desired/applied revision state
- passive workers expose authenticated local agent endpoints; management-side passive orchestration is intentionally minimal in this slice

### 4.3 Listeners

Each listener has:

- `listen_addr`
- `protocol = tcp | udp`
- optional `upstream`

Current listener behavior:

- **TCP + empty upstream** → hostname-routing mode
- **TCP + non-empty upstream** → fixed-forward mode
- **UDP + non-empty upstream** → fixed-forward mode

UDP listeners without an upstream are skipped with a warning.

`":PORT"` syntax is accepted in config and normalized to `0.0.0.0:PORT` before bind.

### 4.4 Routes

Routes are compiled into an ordered routing table.

Accepted fields:

- `host` / `hosts`
- `upstream` / `upstreams`
- `backend` / `backends` (compatibility aliases)
- `middlewares`
- `parsers` (deprecated alias of `middlewares`)
- `strategy`

Normalization rules:

- hosts are lowercased and trimmed
- empty hosts are rejected
- upstream values are trimmed and empty values are rejected
- middleware names are lowercased, trimmed, and `-` is normalized to `_`

Current requirement: **routing routes must declare at least one middleware**. Prism does not auto-insert a default middleware chain when `middlewares` is omitted.

### 4.5 Tunnel config

Tunnel config is split across:

- `tunnel.auth_token`
- `tunnel.auto_listen_services`
- `tunnel.endpoints[]`
- `tunnel.client`
- `tunnel.services[]`

Registered services carry:

- `name`
- `proto`
- `local_addr`
- `route_only`
- `remote_addr`
- `masquerade_host`

Normalization rules:

- empty service names are rejected
- `proto` defaults to `tcp`
- `route_only = true` clears `remote_addr`
- `masquerade_host` is trimmed and lowercased

### 4.6 Runtime knobs

Other important config fields:

- `admin_addr`
- `max_header_bytes`
- `proxy_protocol_v2`
- `buffer_size`
- `upstream_dial_timeout_ms`
- `timeouts.handshake_timeout_ms`
- `timeouts.idle_timeout_ms`
- `reload.enabled`
- `reload.poll_interval_ms`
- `logging.level`
- `logging.format`
- `logging.output`
- `logging.add_source`

Current note: `buffer_size` is preserved as a config knob, but stream proxying still uses `tokio::io::copy_bidirectional`, so the setting does **not** currently tune the actual copy buffer size.

### 4.7 Managed config document

The managed control plane stores a structured runtime document instead of raw TOML/YAML text.

The document currently mirrors the main runtime fields for:

- `listeners`
- `routes`
- runtime knobs (`max_header_bytes`, `proxy_protocol_v2`, `buffer_size`, `upstream_dial_timeout_ms`, `timeouts.*`)
- `tunnel`

Current managed apply model:

- the panel edits this structured document
- the management node validates it through the same normalization path as file config loading
- workers track `desired_revision` and `applied_revision`
- restart-required changes are surfaced explicitly instead of being claimed as hot-reloadable

## 5. Proxy plane

### 5.1 TCP fixed-forward mode

When a TCP listener has a non-empty `upstream`, Prism:

1. accepts the client connection
2. dials the configured upstream
3. optionally writes a PROXY protocol v2 header first
4. proxies bytes bidirectionally until completion or timeout

If the upstream starts with `tunnel:`, Prism resolves it through the tunnel manager instead of dialing TCP directly.

### 5.2 TCP hostname-routing mode

When a TCP listener has an empty `upstream`, Prism enters routing mode.

Current flow:

1. Accept the client connection.
2. Capture bytes from the stream until one of the following happens:
   - a route matches
   - no route can match
   - `max_header_bytes` is reached
   - `handshake_timeout` fires
3. For each route, run the route's middleware chain in **parse** mode against the captured prelude.
4. If middleware extracts a host, match that host against the route's configured patterns.
5. On the first matching route, expand upstream templates using wildcard captures.
6. Order upstream candidates using `strategy`.
7. Dial candidates with failover until one succeeds.
8. Apply any parse-phase prelude override.
9. Run the middleware chain in **rewrite** mode using the selected upstream label.
10. Optionally write a PROXY protocol v2 header.
11. Write the final prelude to the upstream and switch to full bidirectional proxying.

If no route matches, Prism closes the connection.

### 5.2.1 Route matching

Routes are checked in order and **first match wins**.

Pattern support:

- exact match
- `*` wildcard → captures any string
- `?` wildcard → captures one character

Captured wildcard groups are available to upstream templates as `$1`, `$2`, and so on.

### 5.2.2 Upstream ordering

Supported strategies:

- `sequential`
- `random`
- `round-robin`

Ordering selects the candidate order. Actual network failover still happens in the proxy layer, which dials candidates one by one until a connection succeeds.

### 5.2.3 Default port filling

Direct upstreams may omit the port. When that happens, Prism fills the port from the listener that accepted the connection.

### 5.2.4 Masquerade host for tunnel rewrites

When the selected upstream is `tunnel:<service>`, Prism may use `tunnel.services[].masquerade_host` as the label passed into middleware rewrite mode.

This allows rewrite-capable middleware to emit a host/port label that is different from the internal `tunnel:<service>` target and supports chained or branded edge deployments.

### 5.3 UDP fixed-forward mode

UDP listeners currently support fixed forwarding only.

Current behavior:

- Prism maintains lightweight per-client UDP sessions
- each UDP client flow gets its own forwarding task
- optional idle timeout cleanup is applied per UDP flow

Forwarding targets can be:

- a direct UDP socket
- a tunnel service via `tunnel:<service>`

UDP listeners do **not** perform hostname routing.

### 5.4 Timeouts and connection lifetime

Current timeout behavior:

- `timeouts.handshake_timeout_ms` bounds TCP prelude capture in routing mode
- `timeouts.idle_timeout_ms` bounds the lifetime of the bidirectional copy operation
- `upstream_dial_timeout_ms` bounds upstream dial attempts

For UDP listeners, idle timeout is used to reap inactive per-client flows.

### 5.5 PROXY protocol v2

When `proxy_protocol_v2 = true`, Prism prepends a PROXY protocol v2 header on TCP upstream connections before writing any proxied prelude bytes.

This is supported for both fixed-forward and routing TCP paths.

## 6. Middleware model

### 6.1 Distribution format

Prism only loads middleware from **WAT text** files (`.wat`).

Current non-goal: Prism intentionally rejects raw `.wasm` binaries.

### 6.2 Built-in middlewares

The repository currently ships two built-in reference middlewares:

- `minecraft_handshake`
- `tls_sni`

On startup and reload, Prism writes these files into the resolved `middleware_dir` if they do not already exist.

### 6.3 Middleware phases

Middleware is run in two phases:

- **Parse**
  - input: captured prelude bytes
  - output: optional host and optional prelude override
- **Rewrite**
  - input: chosen upstream label plus the current prelude buffer
  - output: optional rewritten prelude

The middleware chain is fail-soft:

- `NeedMoreData` means Prism should keep reading
- `NoMatch` means that middleware or route does not apply
- fatal middleware errors are treated as route-level non-matches so other routes can still win

### 6.4 Middleware ABI (v1)

Each middleware module must export:

- linear memory as `memory`
- function `prism_mw_run(input_len: i32, ctx_ptr: i32) -> i64`

Current return contract:

- `0` → need more data
- `1` → no match
- `-1` → fatal middleware error
- otherwise → packed `(ptr, len)` pointing to an output struct in module memory

Current context struct written by Prism:

- `u32 version`
- `u32 phase` (`0 = parse`, `1 = rewrite`)
- `u32 upstream_ptr`
- `u32 upstream_len`

Current output struct expected by Prism:

- `u32 host_ptr`
- `u32 host_len`
- `u32 rewrite_ptr`
- `u32 rewrite_len`

If `host_len > 0`, Prism reads a routing host.
If `rewrite_len > 0`, Prism reads replacement prelude bytes.

## 7. Tunnel mode

### 7.1 Core model

Tunnel mode is a reverse-connection model:

- a **server-side Prism** accepts tunnel sessions on one or more endpoints
- a **client-side Prism** dials out to the server and registers services
- each proxied TCP or UDP session is carried over a multiplexed substream inside that tunnel session

### 7.2 Transport implementations

Current transport implementations are:

- **tcp** → TCP transport with yamux stream multiplexing
- **udp** → KCP (reliable UDP) transport with yamux stream multiplexing
- **quic** → QUIC bidirectional streams over UDP

QUIC endpoints support either:

- explicit `cert_file` + `key_file`
- auto-generated self-signed certificate when both are empty

### 7.3 Registration protocol

The first stream opened on a tunnel session is the register stream.

Current register header:

- 4-byte magic: `PRRG`
- 1-byte protocol version: `0x01`
- 4-byte length
- JSON payload

Current JSON payload fields:

- `token`
- `services[]`

The server validates the shared token if `tunnel.auth_token` is non-empty.

### 7.4 Proxy stream protocol

For each proxied session, the server opens a fresh substream to the client.

Current proxy stream headers:

- `PRPX` for TCP
- `PRPU` for UDP
- version byte `0x01`
- service name encoded as a Minecraft-style VarInt string

After the header:

- TCP carries raw stream bytes
- UDP carries framed datagrams as `u32be length + payload`

Maximum register payload and datagram frame size are both currently capped at **1 MiB**.

### 7.5 Service ownership semantics

Tunnel services are tracked by the tunnel manager.

Current behavior:

- the **first active registrant** for a given service name becomes the routing owner
- later clients registering the same service name do not replace that owner
- when the owner disconnects, Prism promotes the **oldest remaining active client** for that service

This means:

- `tunnel:<service>` routing is stable while the primary remains connected
- later duplicate registrations may still be used for auto-listen exposure by client/service identity

### 7.6 Auto-listen for services

When `tunnel.auto_listen_services = true`, Prism watches the registered service set and opens server-side listeners for services that declare `remote_addr`.

Current rules:

- `route_only = true` excludes a service from auto-listen
- auto-listen is keyed by `client_id/service`
- TCP and UDP auto-listen are both supported
- UDP auto-listen maintains per-peer flow state with idle timeout cleanup

### 7.7 UDP tunnel limitation

UDP forwarded through a tunnel is transported correctly, but Prism does **not** preserve the original client IP/port at the backend.

The backend sees traffic as originating from the Prism instance that terminates the tunnel flow.

## 8. Admin API, telemetry, and frontend status

### 8.1 Admin API

The admin API is implemented with Axum and exposes:

- `GET /health`
- `GET /metrics`
- `GET /conns`
- `GET /tunnel/services`
- `GET /config`
- `POST /reload`
- `GET /managed/status`
- `GET /managed/nodes`
- `GET /managed/nodes/{node_id}`
- `GET /managed/nodes/{node_id}/config`
- `PUT /managed/nodes/{node_id}/config`
- `POST /managed/worker/sync`
- `GET /managed/worker/status`
- `PUT /managed/worker/config`

Current endpoint behavior:

- `/health` returns `{ "ok": true }`
- `/metrics` returns Prometheus text exposition
- `/conns` returns the current session snapshot
- `/tunnel/services` returns `[]` if no tunnel manager is configured
- `/config` returns the resolved config path
- `/reload` increments and broadcasts a reload sequence number on a watch channel
- `/managed/status` returns the management state path and node count
- `/managed/nodes*` exposes node inventory and desired config snapshots for the panel
- `/managed/worker/sync` is the active worker heartbeat + desired-config sync path
- `/managed/worker/status` and `/managed/worker/config` are worker-agent endpoints used for worker-local inspection/apply

Current operational note: the reload endpoint is **best-effort**. It still returns success even if there are no active receivers.

### 8.2 Admin security posture

Current implementation details:

- the admin router applies `CorsLayer::permissive()`
- the admin router applies `TraceLayer::new_for_http()`
- legacy read endpoints remain unauthenticated
- `/reload` requires bearer auth when panel or worker auth is configured
- management panel endpoints require the configured `panel_token`
- worker sync / worker-agent endpoints require the configured worker auth token

Any deployment that needs tighter admin-plane security should still scope CORS and network exposure externally; the current implementation authenticates writes but does not yet ship origin allowlisting or multi-user auth.

### 8.3 Metrics

Prometheus export is installed through the `metrics` facade.

Metrics currently emitted by the proxy path include:

- `prism_active_connections`
- `prism_connections_total`
- `prism_bytes_ingress_total`
- `prism_bytes_egress_total`
- `prism_route_hits_total{host="..."}`

### 8.4 Managed persistence and frontend status

Management persistence lives in JSON files under the Prism workdir:

- management role: `managed.management.state_file` (default `managed-state.json`)
- worker role: `managed-worker-state.json`

The repository root frontend under `src/` now provides a Prism panel that:

- stores the management base URL + panel bearer token in browser storage
- shows dashboard and node inventory views
- shows desired/applied revision state per node
- edits managed config documents through a structured visual editor rather than raw file text

### 8.5 Embedded frontend (rust-embed)

The admin HTTP server embeds the frontend SPA as static assets via `rust-embed`. This allows a single Docker image (and a single binary) to serve both the API and the management panel.

Build pipeline:

1. `pnpm build` builds the frontend from the single root `vite.config.ts`, which enables TanStack Start [SPA mode](https://tanstack.com/start/latest/docs/framework/react/guide/spa-mode). Output goes to `dist/client/`.
2. Release packaging and the Docker build copy `dist/client/` into `crates/prism/frontend-dist/`.
3. `cargo build` / `cargo build --release` compile the Rust binary with those assets embedded at compile time via `rust-embed`.

The admin router adds a fallback handler that:

- serves exact-match files from the embedded assets (JS, CSS, images, etc.)
- falls back to `_shell.html` for any unmatched path (SPA client-side routing)
- returns 404 if no frontend assets are embedded (for example, if the SPA build artifacts were not copied into `frontend-dist` before compiling)

In debug builds (`cargo build` without `--release`), `rust-embed` reads from the filesystem at runtime, allowing live frontend iteration without recompiling Rust.

## 9. Reload, shutdown, and reliability boundaries

### 9.1 Reload behavior

Prism runs a polling reload loop that watches the config file signature and also listens for manual reload signals.

Current reload behavior:

- reloads the parsed route set
- rebuilds middleware chains
- refreshes TCP runtime knobs (`max_header_bytes`, timeouts, dial timeout, buffer_size, proxy_protocol_v2`)
- updates reload-loop enablement and poll interval
- worker active sync can hot-apply the same runtime-updatable fields from a managed config document

Current restart-required behavior:

- listener topology changes are detected but not applied live
- admin bind changes are not applied live
- tunnel endpoint changes are not applied live
- logging configuration is not reinitialized on reload
- managed worker applies mark these differences as `pending_restart` and preserve restart reasons in worker/management state

### 9.2 Graceful shutdown

Prism listens for:

- `Ctrl-C`
- `SIGTERM` on Unix

On shutdown it:

1. broadcasts a shutdown signal
2. lets long-lived tasks observe that signal and exit
3. drains running tasks
4. applies a hard timeout before aborting any task that still hangs

## 10. Logging

Logging is based on `tracing` and `tracing-subscriber`.

Supported output settings today:

- `stderr`
- `stdout`
- `discard`
- append to a file path

Supported formats today:

- `json`
- `text`

Current design boundary:

- Prism logs to process outputs or a configured file
- Prism does not expose an in-memory log tail endpoint
- Prism does not include built-in distributed tracing export

## 11. Current limitations and explicit non-goals

The following are true in the current implementation and should be documented as such:

- routes do **not** create default listeners
- routing routes require explicit `middlewares`
- `parsers` is only a compatibility alias, not the primary term
- raw `.wasm` middleware loading is disabled; use `.wat`
- UDP listeners do not support hostname routing
- `buffer_size` is not yet wired into the actual copy buffer implementation
- the admin router remains CORS-permissive even though managed write endpoints now require bearer auth
- active worker sync is implemented end-to-end, but management-side passive orchestration is still limited to worker-agent endpoint exposure
- managed auth currently uses shared bearer tokens rather than per-node secrets or RBAC
