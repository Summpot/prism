# Technical Design Document: Prism

## 1. Executive Summary

Prism is a lightweight, high-performance reverse proxy designed primarily for the Minecraft protocol.

* For **TCP**, Prism can multiplex incoming traffic from one or more listening ports to multiple backend servers based on a hostname found in the connection's initial bytes (Minecraft handshake or TLS SNI).
* For **UDP**, Prism can expose one or more listening ports and forward datagrams to an upstream (directly or via tunnel). This enables proxying UDP-based games (for example, Bedrock-like protocols) where hostname-based routing is not available.

Prism can also operate in a **reverse-connection ("tunnel")** mode similar to frp: a tunnel client running on a private network establishes an outbound connection to a tunnel server. When a route targets a tunnel service, Prism forwards the incoming TCP session over that existing reverse connection, allowing access to backends that have no public IP.

While Minecraft handshake parsing is the primary use-case, Prism also supports extracting hostnames from standard TLS SNI so the same routing stack can be reused for other TCP frontends.

The system is designed with a **test-first architecture**, ensuring that core logic (protocol parsing, routing, traffic shaping) can be verified via unit tests without requiring active network sockets or running Minecraft server instances.

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
  * **TCP**: wraps the standard `net.Listener`. It accepts connections and spawns a `SessionHandler` for each.
  * **UDP**: wraps a `net.PacketConn`. It reads datagrams and hands them to a packet handler which maintains lightweight per-client sessions.
* **Testability**:
* The `Listener` accepts a `ConnectionHandler` interface.
* This allows integration tests to simulate a flood of connections without spawning real goroutines for the entire logic stack.

**Multi-listener**:

Prism can run multiple listeners at once (different ports and/or protocols). Each listener is configured independently (e.g., TCP hostname-routing on `:25565` and UDP forwarding on `:19132`).



### 3.2. `SessionHandler` (Orchestrator)

* **Responsibility**: Manages the lifecycle of a single client connection. It does not contain business logic but orchestrates calls to the Parser, Router, and Proxy components.
* **Context Management**: Owns the `context.Context` for the connection, handling timeouts and cancellation signals.

### 3.3. `RoutingHeaderParser` (Pure Logic, Pluggable)

* **Responsibility**: Extracts a routing hostname from the first bytes of a TCP stream.
* **Design**: Stateless per call; no direct dependency on `net.Conn`.
* **Input**: `[]byte` (captured initial bytes).
* **Output**: `(host string, error)` where errors are classified as:
  * `need more data` (caller should read more bytes)
  * `no match` (parser does not apply to this stream)
  * fatal error

* **Built-in implementations**:
  * Minecraft handshake hostname extractor
  * TLS ClientHello SNI hostname extractor

* **Plugin support (WASM)**:
  * Parsers can be loaded from WebAssembly modules to avoid hardcoding parsing logic in Go.
  * WASM is used only on the connection prelude; the hot path (byte bridging) remains native.

* **Testability**:
  * Since parsing is `[]byte` -> `host`, unit tests can provide deterministic byte slices.





### 3.4. `Router` (Routing Layer)

* **Responsibility**: Maps a hostname string to an upstream address.
* **Design**:
  * Supports exact matches and wildcard matches.
  * Thread-safe reads; atomic writes.
  * Route tables can be updated at runtime as part of config hot-reload.

* **Upstream address format**:
  * Upstream targets are treated as TCP dial addresses (e.g. `host:port`, `ip:port`, `[ipv6]:port`).
  * A port may be omitted (e.g. `backend.example.com`), in which case the proxy prefers the port from a Minecraft handshake when available and otherwise falls back to the listener port.
  * Tunnel upstreams use the prefix `tunnel:` (for example `tunnel:home-mc`). In this case the target is **not** treated as a TCP dial address; it is resolved via the tunnel registry maintained by the tunnel server.


* **Testability**:
  * Defined as an interface `UpstreamResolver`.
  * Tests can inject a `MockRouter` that returns deterministic results, decoupling routing logic from configuration file parsing.



### 3.5. `UpstreamDialer` (Network Abstraction)

* **Responsibility**: Establishes the connection to the backend Minecraft server.
  * In tunnel mode, the dialer may return a `net.Conn` backed by a multiplexed tunnel stream instead of a direct TCP socket.
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

## 4. Admin & Telemetry Module

This module runs a separate HTTP server to provide observability. It interacts with the core components via thread-safe data stores.

### 4.1. `MetricsCollector`

* **Responsibility**: A central repository for atomic counters and gauges.
* **Metrics Tracked**:
* `active_connections` (Gauge)
* `total_connections_handled` (Counter)
* `bytes_ingress` / `bytes_egress` (Counter)
* `route_hits` (Map/Vector: Hostname -> Count)


* **Design**:
* Uses atomic operations (`sync/atomic`) for minimal performance impact.
* Decoupled from the HTTP handlers.



### 4.2. `AdminServer`

* **Responsibility**: Exposes the data from `MetricsCollector` via HTTP/JSON.
* **Endpoints**:
* `GET /health`: Returns 200 OK if the listener is up.
* `GET /metrics`: Returns the JSON dump of the `MetricsCollector`.
* `GET /conns`: Returns a snapshot list of current active sessions (Client IP -> Target Host).
* `GET /logs?limit=N`: Returns the most recent log lines from an in-memory ring buffer (if enabled).
* `POST /reload`: Triggers a configuration reload and atomically swaps the snapshot used for new connections.



### 4.3. Logging

Prism uses the Go standard library structured logging package `log/slog`.

**Goals**:

* Make debugging and postmortem analysis practical (why was a connection dropped? which upstream was chosen?).
* Keep the hot path fast by default (no per-connection logs at `info` level).
* Produce machine-readable logs by default.

**Design**:

* A process-wide logger is constructed at startup from `config.logging`.
* Output format is `json` by default, with an optional `text` mode for local debugging.
* Log level is runtime-adjustable on config reload; changing output/format/source reporting requires restart.
* Log records should be structured with consistent keys where relevant:
  * `sid`: session identifier (process-unique)
  * `client`: client remote address
  * `host`: routed hostname
  * `upstream`: upstream address
  * `err`: error value

**Admin log tail**:

* When `config.logging.admin_buffer.enabled` is true, Prism tees formatted log lines into a bounded in-memory ring buffer.
* The admin server can expose this buffer via `GET /logs?limit=N` to quickly inspect recent activity without filesystem access.
* This is intended for debugging, not long-term retention.



---

## 5. Testability Strategy

The project will strictly adhere to the following testing pyramid:

### 5.1. Unit Tests (Logic Verification)

* **Parser Tests**: Use "Table-Driven Tests" with various raw byte inputs (valid handshake, partial packet, malformed varint) to verify the `ProtocolParser` extracts the correct hostname.
* **Router Tests**: Verify wildcard matching logic (e.g., ensuring `*.example.com` matches `play.example.com`).
* **Proxy Protocol Tests**: Verify the binary encoding of the PROXY v2 header matches the spec exactly.

### 5.2. Integration Tests (Component Interaction)

* **Mock Network Test**: Use `net.Pipe()` to simulate a client connecting to Prism, and Prism connecting to a backend. Send a handshake message into the client pipe and verify it arrives at the backend pipe.
* **Hot Reload Test**: Start the router, resolve a host, modify the underlying config source, trigger reload, and verify the host resolves to a new address.

### 5.3. End-to-End (E2E) Tests (Optional)

* Spin up a real TCP listener locally and connect with a real TCP client (Go client, not MC client) to verify socket handling, timeouts, and graceful shutdown.

---

## 6. Configuration & Reliability

### 6.1. Configuration Model

* The configuration is defined as a Go struct.
* A `ConfigProvider` interface loads this struct. This allows the config to be loaded from a file (Production) or a static string/struct (Testing).
* **Config file formats & naming**:
  * Prism supports **TOML** and **YAML** configuration files.
  * **Config path resolution** (highest precedence first):
    * CLI flag: `-config /path/to/prism.toml`
    * Environment variable: `PRISM_CONFIG=/path/to/prism.toml`
    * Auto-discovery in the current working directory: `prism.toml` > `prism.yaml` > `prism.yml`
    * OS-specific default user config path: `${UserConfigDir}/prism/prism.toml` (where `UserConfigDir` comes from `os.UserConfigDir()`)
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
  * `logging.format`, `logging.output`, `logging.add_source`, and `logging.admin_buffer.*` require restart.

### 6.2. Buffer Management

* A `BufferPool` interface wraps `sync.Pool`.
* This facilitates testing memory behavior and ensures the application doesn't leak memory during high concurrency.

### 6.3. Graceful Shutdown

* The application will listen for `SIGINT`/`SIGTERM`.
* Upon signal:

    1. Close the `Listener` (stop accepting new connections).
    2. Wait for existing `SessionHandlers` to finish (with a hard timeout context).
    3. Flush final metrics.



---

## 7. WASM Routing Parser ABI (v1)

Prism's WASM routing header parser interface is intentionally tiny to keep overhead low.

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
