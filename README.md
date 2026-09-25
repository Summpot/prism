# Prism

High-performance, lightweight L4 reverse proxy and multiplexed tunneling engine written in Rust.

Prism combines the speed and memory safety of Rust with a WebAssembly-powered routing pipeline and modern multiplexed reverse tunneling. It routes arbitrary TCP/UDP traffic dynamically, exposes private services across firewalls without public IPs, and provides centralized management through an administrative REST API and cross-platform desktop GUI.

---

## Key Features

- **Dynamic L4 Reverse Proxy**: Multi-port TCP/UDP listeners with routing based on application handshakes via WebAssembly middlewares (e.g. Minecraft handshake, TLS SNI).
- **Wildcard & Regex Routing**: Route matching with capture group substitution (`$1`, `$2`), load balancing (`round-robin`, `random`, `sequential`), and HAProxy PROXY protocol v2 support.
- **Multiplexed Reverse Tunneling**: Expose internal services to public edge nodes using frp-like reverse tunnels over **QUIC, WebTransport (HTTP/3), WebSocket (WS/WSS), KCP, TCP, or UDP**.
- **Stream Optimizer**: Continuous real-time Zstandard (zstd) stream compression with adaptive batching, sliding window configuration, and shared dictionary support.
- **Local Service Discovery**: Automatic mDNS advertisement (`*.prism.local`) and Minecraft LAN multicast broadcast reflection (`224.0.2.60:4445`) for zero-config client connections.
- **Automated TLS & DNS**: Built-in ACME (RFC 8555 / Let's Encrypt / ZeroSSL) with Cloudflare DNS-01 challenge and automatic RFC 9460 HTTPS/SVCB record publication.
- **Managed Clustering**: Deploy as `standalone`, `management` (control plane), or `worker` (edge agent) with real-time config synchronization.
- **Admin API & Desktop App**: Built-in administrative management API and Tauri v2 cross-platform desktop client with GitHub OAuth and deep linking (`prism://`).

---

## Quick Start

### Docker Compose

Run Prism as a lightweight headless container:

```yaml
services:
  prism:
    image: ghcr.io/summpot/prism:latest
    # Or build locally:
    # build: .
    restart: unless-stopped
    environment:
      PRISM_CONFIG: /etc/prism/prism.toml
    volumes:
      - ./prism.toml:/etc/prism/prism.toml:ro
      - prism-data:/var/lib/prism
    ports:
      - "8080:8080" # Admin API
      - "7000:7000" # Tunnel endpoint
      - "25565:25565" # Public proxy listener
volumes:
  prism-data:
```

### Pre-built Binary

```bash
# Run headless server
./prism --headless --config prism.toml

# Launch desktop GUI (default when compiled with desktop feature)
./prism --gui
```

---

## Configuration

Prism supports **TOML** and **YAML** configuration (`prism.toml`, `prism.yaml`). Configuration files are validated against the schema defined in [`prism.schema.json`](./prism.schema.json).

### 1. Reverse Proxy with WASM Host Routing

Route connections on port `25565` to different backend servers based on the incoming hostname:

```toml
admin_addr = "127.0.0.1:8080"
buffer_size = 32768
upstream_dial_timeout_ms = 5000

# Proxy listener
[[listeners]]
listen_addr = ":25565"
protocol = "tcp"

# Hostname routing table
[[routes]]
host = "mc.example.com"
upstream = "127.0.0.1:25566"
middlewares = ["minecraft"]

[[routes]]
host = "*.play.example.com"
upstreams = ["10.0.0.10:25565", "10.0.0.11:25565"]
strategy = "round-robin"
middlewares = ["minecraft"]

# TLS SNI routing example
[[routes]]
host = "secure.example.com"
upstream = "127.0.0.1:8443"
middlewares = ["tls_sni"]
```

### 2. Reverse Tunnel (Exposing Private Services)

#### Edge Server (`prism-server.toml`)

Listen for incoming tunnel connections over QUIC and TCP, and auto-expose registered service ports:

```toml
admin_addr = "127.0.0.1:8080"

[tunnel]
auth_token = "your-secret-token"
auto_listen_services = true

[[tunnel.endpoints]]
listen_addr = ":7000"
transport = "tcp"

[[tunnel.endpoints]]
listen_addr = ":7001"
transport = "quic"
```

> **Note:** To expose the admin server on all network interfaces (e.g. `admin_addr = "0.0.0.0:8080"`), you must explicitly set `admin_allow_remote = true` and configure an admin credential (`panel_token`, `legacy_token`, or GitHub OAuth).

#### Private Host / Connector (`prism-connector.toml`)

Connect out to the edge server and publish local services:

```toml
[tunnel.connector]
server_addr = "relay.example.com:7001"
transport = "quic"
auth_token = "your-secret-token"

[[tunnel.services]]
name = "minecraft-srv"
proto = "tcp"
local_addr = "127.0.0.1:25565"
remote_addr = ":25565" # Public port exposed on edge server

# Optional continuous zstd stream compression
[tunnel.services.optimizer]
enabled = true
adaptive_flush = true
```

### 3. Admin API & Authentication

Prism includes an administrative management API. You can secure access using a static bearer token (`panel_token`) or GitHub OAuth.

#### Admin API with Token (`prism.toml`)

```toml
admin_addr = "127.0.0.1:8080"

[auth]
panel_token = "your-admin-panel-secret"
```

#### Admin API with GitHub OAuth

```toml
admin_addr = "127.0.0.1:8080"

[auth.github]
enabled = true
client_id = "your-github-client-id"
client_secret = "your-github-client-secret"
redirect_uri = "http://127.0.0.1:8080/api/auth/github/callback"
admin_users = ["your-github-username"]
```

---

## CLI & Environment Variables

| Flag                      | Env Var                | Default                    | Description                                                                                                  |
| :------------------------ | :--------------------- | :------------------------- | :----------------------------------------------------------------------------------------------------------- |
| `--config <PATH>`         | `PRISM_CONFIG`         | Auto-detected              | Path to `.toml` / `.yaml` config file. Auto-searches CWD then OS default (`/etc/prism/prism.toml` on Linux). |
| `--workdir <PATH>`        | `PRISM_WORKDIR`        | `/var/lib/prism` (Linux)   | Runtime working directory for state, certs, and SQLite DB.                                                   |
| `--middleware-dir <PATH>` | `PRISM_MIDDLEWARE_DIR` | `<config_dir>/middlewares` | Directory containing `.wat` / `.wasm` middleware files.                                                      |
| `--headless`              | -                      | `false`                    | Run headless without GUI even if desktop support is compiled in.                                             |
| `--gui`                   | -                      | `false`                    | Force launch Tauri desktop GUI window.                                                                       |

---

## WebAssembly Middlewares & Companion Agent

- **WASM Middlewares**: Middleware modules (located in `middlewares/` or `<config_dir>/middlewares/`) run inside sandboxed Wasmtime runtimes to parse initial protocol handshakes (`Parse` phase) and optionally rewrite headers (`Rewrite` phase). Standard modules include `minecraft.wat` and `tls_sni.wat`.
- **Minecraft Java Companion Agent**: `agent/prism-agent.jar` is a zero-dependency Java Agent that transparently captures the server's RSA keypair in Minecraft online mode and pushes it to Prism, allowing secure protocol interception and routing.

---

## Building from Source

### Prerequisites

- [Rust](https://rustup.rs/) (2024 edition, MSRV 1.85+)
- [Node.js](https://nodejs.org/) (v22+) and [pnpm](https://pnpm.io/) (v10+)

### Build Headless CLI / Server

```bash
cargo build --release -p prism --no-default-features
```

The binary will be located at `target/release/prism`.

### Build Desktop GUI (Tauri v2)

```bash
# Install frontend dependencies
pnpm install

# Build desktop application
pnpm build:desktop
```

---

## License

This project is licensed under the [MIT License](LICENSE).
