# Technical Design Document: Prism

## 1. Executive Summary

Prism is a lightweight, high-performance reverse proxy designed primarily for the Minecraft protocol.

* For **TCP**, Prism can multiplex incoming traffic from one or more listening ports to multiple backend servers based on a hostname found in the connection's initial bytes (Minecraft handshake or TLS SNI).
* For **UDP**, Prism can expose one or more listening ports and forward datagrams to an upstream (directly or via tunnel). This enables proxying UDP-based games (for example, Bedrock-like protocols) where hostname-based routing is not available.

Prism can also operate in a **reverse-connection ("tunnel")** mode similar to frp: a tunnel client running on a private network establishes an outbound connection to a tunnel server. When a route targets a tunnel service, Prism forwards the incoming TCP session over that existing reverse connection, allowing access to backends that have no public IP.

While Minecraft handshake parsing is the primary use-case, Prism also supports extracting hostnames from standard TLS SNI so the same routing stack can be reused for other TCP frontends.

The system is designed with a **test-first architecture**, ensuring that core logic (protocol parsing, routing, traffic shaping) can be verified via unit tests without requiring active network sockets or running Minecraft server instances.

**Implementation note (current codebase):** Prism is implemented in **Rust**.

* CLI parsing uses **clap**.
* WebAssembly routing parsers execute via **wasmer** (Singlepass compiler).
* The default routing parsers (`minecraft_handshake` and `tls_sni`) are shipped as embedded **WAT** sources (WebAssembly text format) that implement the same ABI as third-party parsers and can be replaced at runtime via configuration. Prism compiles WAT to WASM at runtime and **does not load raw `.wasm` binaries**.

---

## 2. Architecture Overview

The system follows a **Layered Architecture**. Each layer communicates with the next via defined interfaces, allowing for easy mocking and stubbing during testing.

### High-Level Data Flow

1. **Transport Layer**: Accepts raw TCP connections and/or UDP datagrams.
2. **Header Parsing Layer**: Reads/peeks the initial bytes to extract a routing hostname.
3. **Routing Layer**: Resolves the destination address based on the extracted hostname.
4. **(Optional) Tunnel Layer**: If the resolved upstream is a tunnel service, opens a tunneled stream to the registered tunnel client instead of dialing a direct upstream address.
5. **Proxy Layer**: Establishes the upstream connection (direct TCP or tunneled stream) and pipes data between client and server.

---

## 3. Core Component Design

### 3.1. `Listener` (Transport Layer)

* **Responsibility**:
  * **TCP**: accepts connections (e.g. `tokio::net::TcpListener`) and spawns a `SessionHandler` for each.
  * **UDP**: reads datagrams (e.g. `tokio::net::UdpSocket`) and hands them to a packet handler which maintains lightweight per-client sessions.
* **Testability**:
* The `Listener` accepts a `ConnectionHandler` interface.
* This allows integration tests to simulate a flood of connections without spawning real goroutines for the entire logic stack.

**Multi-listener**:

Prism can run multiple listeners at once (different ports and/or protocols). Each listener is configured independently (e.g., TCP hostname-routing on `:25565` and UDP forwarding on `:19132`).



### 3.2. `SessionHandler` (Orchestrator)

* **Responsibility**: Manages the lifecycle of a single client connection. It does not contain business logic but orchestrates calls to the Parser, Router, and Proxy components.
* **Cancellation/Timeouts**: Owns the per-connection cancellation scope and applies configured timeouts (handshake, idle, dial). In Rust this is modeled via task cancellation and `tokio::time::timeout` (and may use a cancellation token internally).

### 3.3. `RoutingHeaderParser` (Pure Logic, Pluggable)

* **Responsibility**: Extracts a routing hostname from the first bytes of a TCP stream.
* **Design**: Stateless per call; no direct dependency on `net.Conn`.
* **Input**: `[]byte` (captured initial bytes).
* **Output**: `(host string, error)` where errors are classified as:
  * `need more data` (caller should read more bytes)
  * `no match` (parser does not apply to this stream)
  * fatal error

* **Implementations**:
  * All routing header parsers are **WASM modules** implementing the ABI described in section 7.
  * In configuration and on disk, Prism loads these modules from **WAT text** (`.wat`) only.
  * Prism ships two default parsers as embedded WAT modules:
    * `minecraft_handshake`
    * `tls_sni`
  * These defaults are not "special" at runtime: they can be replaced by providing a different module path in configuration.

* **Plugin support (WASM)**:
  * Parsers can be loaded from WebAssembly modules to avoid hardcoding parsing logic in the host language.
  * Prism ships the default parsers as embedded WAT modules, so the default routing behavior uses the same ABI as external plugins.
  * WASM is used only on the connection prelude; the hot path (byte bridging) remains native.

* **Testability**:
  * Since parsing is `[]byte` -> `host`, unit tests can provide deterministic byte slices.





### 3.4. `Router` (Routing Layer)

* **Responsibility**: Maps a hostname string to an upstream address.
* **Design**:
  * Routes are an **ordered list**; the router checks them in order and the **first match wins**.
    * Operationally: put more specific patterns earlier.
  * Host patterns support exact matches and glob-like wildcards:
    * `*` matches any string (captured as a group)
    * `?` matches any single character (captured as a group)
  * Wildcard patterns produce capture groups which can be substituted into upstream templates using `$1`, `$2`, ...
  * A single route can target **one or more upstreams**.
    * When multiple upstreams are configured, a simple load-balancing strategy chooses the candidate order (for example `sequential`, `random`, `round-robin`).
    * The proxy layer performs **dial failover** by trying upstreams in that order until one succeeds.
  * Thread-safe reads; atomic updates.
  * Route tables can be updated at runtime as part of config hot-reload.

* **Upstream address format**:
  * Upstream targets are treated as TCP dial addresses (e.g. `host:port`, `ip:port`, `[ipv6]:port`).
  * A port may be omitted (e.g. `backend.example.com`), in which case the proxy prefers the port from a Minecraft handshake when available and otherwise falls back to the listener port.
  * Tunnel upstreams use the prefix `tunnel:` (for example `tunnel:home-mc`). In this case the target is **not** treated as a TCP dial address; it is resolved via the tunnel registry maintained by the tunnel server.

* **Optional protocol-aware fast paths**:
  * For Minecraft status (ping) traffic, Prism may serve a cached status response for a route when configured to do so (TTL-based).
  * The cache is keyed by upstream + protocol version and uses request coalescing to avoid stampedes under concurrent pings.


* **Testability**:
  * Defined as an interface `UpstreamResolver`.
  * Tests can inject a `MockRouter` that returns deterministic results, decoupling routing logic from configuration file parsing.



### 3.5. `UpstreamDialer` (Network Abstraction)

* **Responsibility**: Establishes the connection to the backend Minecraft server.
  * In tunnel mode, the dialer may return a stream backed by a multiplexed tunnel channel instead of a direct TCP socket.
* **Testability**:
* Instead of calling `net.Dial` directly, the proxy uses a `Dialer` interface.
* **Mocking**: In tests, a `MockDialer` can return a `net.Pipe()`. This connects the "client" side of the test directly to a "mock backend" in memory, allowing end-to-end traffic simulation without real TCP overhead.



### 3.6. `ProxyBridge` (Data Layer)

* **Responsibility**: Handles the bidirectional byte copying between Client and Upstream.
* **Features**:
* **PROXY Protocol v2 Injection**: Inserts the IP preservation header before the first byte of upstream traffic.
* **Buffer Pooling**: Uses a shared interface for buffer acquisition to reduce GC pressure.


* **Testability**:
* Logic operates on `io.ReadWriter`, not `net.TCPConn`.
* Unit tests can verify that the PROXY Protocol header is correctly prepended by inspecting the write buffer of the mock upstream.



---

## 8. Tunnel mode (reverse connection)

Tunnel mode is inspired by frp, but implemented with a much smaller surface area.

Prism is a single binary (`prism`) that can run one or both roles depending on configuration:

* **Proxy server role**: runs one or more proxy listeners (`listeners`) and (optionally) the admin plane (`admin_addr`).
* **Tunnel server role**: runs one or more tunnel endpoints (`tunnel.endpoints`) and maintains a registry of registered services.
* **Tunnel client role**: runs a tunnel client loop (`tunnel.client`) that dials a remote tunnel server over an outbound connection, registers one or more services (`tunnel.services`), and forwards tunneled streams to local TCP backends.

Role enablement is inferred from configuration:

* Proxy server role is enabled when `listeners` has one or more entries (or when `routes` is non-empty, in which case a default TCP listener on `:25565` is added).
* Tunnel server role is enabled when `tunnel.endpoints` has one or more entries.
* Tunnel client role is enabled when `tunnel.client.server_addr` is set and `tunnel.services` is non-empty.

### 8.2. Tunnel registry and routing

* The tunnel client registers one or more **services**.
  * At minimum: `name -> local_addr` (used by the client role to dial a local backend).
  * Optionally: a service can request a **remote listener** (protocol + listen address) to be opened on the server side (`remote_addr`).
  * Optionally: a service can be marked `route_only=true` to indicate it should only be used as a routing target (`tunnel:<service>`) and never exposed as a server-side listener.
* The tunnel server maintains an in-memory registry mapping `service name -> active client session`.
* A route whose upstream is `tunnel:<service>` is forwarded through the active client session that registered that service.

Service name conflicts:

* If multiple tunnel clients register the same service `name`, Prism keeps the **first** active registrant as the routing target for `tunnel:<service>`.
* Later registrations with the same `name` do **not** override routing. They can still be exposed by **port** via `remote_addr` + auto-listen.

### 8.2.1. frp-like "auto listen" for services

When enabled on the tunnel server role, Prism will automatically open server-side listeners for any registered services that specify a remote listen address.

Services marked `route_only=true` are excluded from auto-listen (and `remote_addr` is ignored/invalid for them).

This matches frp-style behavior:

* A service can be exposed by **port** (TCP/UDP) without requiring a hostname route.
* If a service has no matching entry in `routes`, it can still be reachable through its configured remote listener.

Notes / limitations:

* For **UDP over tunnel**, Prism forwards datagrams over a framed stream inside the tunnel session.
* UDP forwarding does **not** preserve the original client IP/port at the backend (the backend sees the tunnel client's source address), similar to typical UDP reverse-proxying constraints.

### 8.3. Transport protocols (server <-> client)

The tunnel link between the client role and the server role supports multiple transport protocols:

* `tcp`: a single TCP connection with stream multiplexing.
* `udp`: a reliable UDP-based connection (KCP) with stream multiplexing.
* `quic`: QUIC with native stream multiplexing.

Only the tunnel **transport** is affected by this choice; the Prism data plane remains a TCP listener.

To support multiple transports simultaneously (similar to frp's server), Prism can run multiple tunnel endpoints at the same time via `tunnel.endpoints`.

### 8.4. Multiplexing model

* One long-lived tunnel connection is established from the client role to the server role.
* Within that connection, multiple independent streams are opened:
  * a short control stream used for registration
  * one stream per proxied TCP session

### 8.5. Authentication

Tunnel mode supports a shared secret token. The tunnel client must present the token during registration; otherwise the tunnel server rejects the tunnel connection.

### 8.6. Tunnel wire protocol (v1)

Prism's tunnel protocol is intentionally small and compatible across implementations.

**Register stream (client → server, first stream in a session)**

* The first stream opened on a tunnel session is the **register** stream.
* Header:
  * 4 bytes ASCII magic: `PRRG` ("Prism Reverse Register")
  * 1 byte version: `0x01`
  * 4 bytes big-endian length $N$
  * $N$ bytes JSON payload (cap: 1 MiB)

Payload (JSON):

* `token: string` (optional shared secret)
* `services: []service`

Service fields:

* `name: string`
* `proto: "tcp" | "udp"` (defaults to `tcp`)
* `local_addr: string` (used by tunnel client to dial the local backend)
* `route_only: bool` (when true, the service is only reachable via `tunnel:<name>` routing)
* `remote_addr: string` (optional; requests server-side auto-listen exposure; ignored when `route_only=true`)

Implementations should normalize the request (trim whitespace, lowercase `proto`, and force `remote_addr=""` when `route_only=true`).

**Proxy stream (server → client, one stream per proxied session)**

The tunnel server opens a new stream to the client for each proxied session.

Header:

* 4 bytes ASCII magic:
  * `PRPX` for TCP streams ("Prism Reverse Proxy")
  * `PRPU` for UDP streams
* 1 byte version: `0x01`
* Service name encoded as a Minecraft-style string: VarInt length + UTF-8 bytes

After the header:

* **TCP**: raw byte stream (the proxy simply bridges bytes).
* **UDP**: datagram framing over a stream:
  * each datagram is `u32be length` + `payload`
  * per-datagram cap: 1 MiB

## 4. Admin & Telemetry Module

This module runs a separate HTTP server to provide observability. It interacts with the core components via thread-safe data stores.

### 4.1. Metrics

Prism publishes metrics using the Rust [`metrics`](https://crates.io/crates/metrics) facade.

* **Export format**: Prometheus text exposition.
* **Collection model**: pull-based scraping.

Prism exposes metrics via the admin plane at:

* `GET /metrics` (Prometheus format)

Typical deployment:

* Prism logs are written to stdout/stderr.
* Vector (or another log shipper) captures container stdout and forwards to a log store (for example Quickwit).
* VictoriaMetrics (or Prometheus) scrapes `GET /metrics` directly.

Tracked metrics (names are subject to change, but intent is stable):

* `prism_active_connections` (gauge)
* `prism_connections_total` (counter)
* `prism_bytes_ingress_total` / `prism_bytes_egress_total` (counters)
* `prism_route_hits_total{host="..."}` (counter)



### 4.2. `AdminServer`

* **Responsibility**: Exposes health, metrics, and operational snapshots.
* **Endpoints**:
* `GET /health`: Returns 200 OK if the listener is up.
* `GET /metrics`: Returns Prometheus text exposition.
* `GET /conns`: Returns a snapshot list of current active sessions (Client IP -> Target Host).
* `POST /reload`: Triggers a configuration reload and atomically swaps the snapshot used for new connections.
* `GET /tunnel/services`: Returns a snapshot of currently registered tunnel services (when tunnel server role is enabled).



### 4.3. Logging

Prism uses Rust structured logging (`tracing` + `tracing-subscriber`).

**Goals**:

* Make debugging and postmortem analysis practical (why was a connection dropped? which upstream was chosen?).
* Keep the hot path fast by default (no per-connection logs at `info` level).
* Produce machine-readable logs by default.

Prism intentionally does **not** export OpenTelemetry data and does **not** collect distributed traces.

**Design**:

* A process-wide logger is constructed at startup from `config.logging`.
* Output format is `json` by default, with an optional `text` mode for local debugging.
* Log level is runtime-adjustable on config reload; changing output/format/source reporting may require restart.
* Log records should be structured with consistent keys where relevant:
  * `sid`: session identifier (process-unique)
  * `client`: client remote address
  * `host`: routed hostname
  * `upstream`: upstream address
  * `err`: error value

**Implementation guidance (logging on the hot path)**:

* Avoid per-connection logs at `info` by default; prefer `debug` for per-session details.
* When building non-trivial fields, guard with `tracing::enabled!(Level::DEBUG)` to avoid allocations when disabled.
* Do not log raw captured handshake bytes at `info`/`warn`. If needed for deep debugging, log only lengths/counts or keep raw data behind `debug` with an explicit justification.

**No in-process log storage**:

* Prism does **not** maintain an in-memory log ring buffer.
* Prism does **not** provide a "tail logs" endpoint backed by process memory.

**External log pipelines**:

* Prism writes logs to stdout/stderr (JSON by default).
* A log shipper (for example Vector) captures stdout and forwards logs to your backend.

### 4.4. Log viewing in the frontend

The Prism frontend does not read logs from Prism's process memory.

* The UI can link to (or embed) an external observability UI configured by the user.
* Optionally, the UI can query the user's log backend directly if that backend provides an HTTP query API and is reachable from the browser environment.



---

## 5. Testability Strategy

The project will strictly adhere to the following testing pyramid:

### 5.1. Unit Tests (Logic Verification)

* **Parser Tests**: Use "Table-Driven Tests" with various raw byte inputs (valid handshake, partial packet, malformed varint) to verify the `ProtocolParser` extracts the correct hostname.
* **Router Tests**: Verify wildcard matching logic (e.g., ensuring `*.example.com` matches `play.example.com`).
* **Proxy Protocol Tests**: Verify the binary encoding of the PROXY v2 header matches the spec exactly.

### 5.2. Integration Tests (Component Interaction)

* **Mock Network Test**: Use an in-memory duplex stream (for example `tokio::io::DuplexStream`) to simulate a client connecting to Prism, and Prism connecting to a backend. Send a handshake message into the client side and verify it arrives at the backend side.
* **Hot Reload Test**: Start the router, resolve a host, modify the underlying config source, trigger reload, and verify the host resolves to a new address.

### 5.3. End-to-End (E2E) Tests (Optional)

* Spin up a real TCP listener locally and connect with a real TCP client (for example `nc`, `curl --raw` for TLS SNI testing, or a small Rust test client) to verify socket handling, timeouts, and graceful shutdown.

---

## 6. Configuration & Reliability

### 6.1. Configuration Model

* The configuration is defined as a Rust struct (Serde-deserializable).
* A `ConfigProvider` interface loads this struct. This allows the config to be loaded from a file (Production) or a static string/struct (Testing).
* **Config file formats & naming**:
  * Prism supports **TOML** and **YAML** configuration files.
  * **Config path resolution** (highest precedence first):
    * CLI flag: `--config /path/to/prism.toml`
    * Environment variable: `PRISM_CONFIG=/path/to/prism.toml`
    * Auto-discovery in the current working directory: `prism.toml` > `prism.yaml` > `prism.yml`
    * OS-specific default user config path: `${ProjectConfigDir}/prism.toml` (derived from `directories::ProjectDirs` in Rust)
  * When multiple `prism.*` files are present in a discovery directory, precedence is: `prism.toml` > `prism.yaml` > `prism.yml`.
  * JSON is intentionally not supported because it cannot contain comments and Prism configs are expected to be annotated.
  * If the resolved config file path does not exist, Prism will **create a runnable default config file** at that path and continue starting (default: tunnel server on `:7000/tcp`).
* **Zero-downtime config reload**:
  * Prism can reload configuration without stopping the TCP listener.
  * Existing sessions continue with the configuration snapshot they started with.
  * New connections use the latest configuration snapshot.
  * Not all fields can be reloaded safely (e.g., listen address/admin address require a restart); reloadable fields include routes, timeouts, dial timeouts, buffer sizing and header parser selection.

* **Logging reload semantics**:
  * `logging.level` is reloadable.
  * `logging.format`, `logging.output`, `logging.add_source` may require restart depending on the sink implementation.

### 6.2. Buffer Management

* Prism currently relies on Tokio's `copy_bidirectional` for stream proxying, which uses internal buffering.
* The config field `buffer_size` is kept as a tuning knob for future work (e.g. switching to a manual copy loop with a reusable `Vec<u8>` buffer pool) but may not affect behavior yet.

### 6.3. Graceful Shutdown

* The application will listen for `SIGINT`/`SIGTERM`.
* Upon signal:

    1. Close the `Listener` (stop accepting new connections).
    2. Wait for existing `SessionHandlers` to finish (with a hard timeout context).
    3. Flush final metrics.



---

## 7. WASM Routing Parser ABI (v1)

Prism's WASM routing header parser interface is intentionally tiny to keep overhead low.

### Module distribution format

* Prism expects routing parser modules to be provided as **WAT text** (`.wat`).
* Prism compiles WAT to WASM at runtime (via Wasmer) and intentionally **rejects loading raw `.wasm` binaries**.

### Memory contract

* Module must export linear memory as `memory`.
* Prism writes the captured prelude bytes into module memory starting at offset `0`.

### Exported function

Module must export:

* `prism_parse(input_len: i32) -> i64`

Return values:

* `0`  : need more data
* `1`  : no match (this parser does not apply)
* `-1` : fatal parse error
* otherwise: packed `(ptr,len)` pointing at the hostname bytes in module memory:
  * lower 32 bits: `ptr` (u32)
  * upper 32 bits: `len` (u32)

The hostname bytes must remain valid until the next call to `prism_parse` on the same module instance.
