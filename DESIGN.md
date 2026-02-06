# Technical Design Document: Prism

## 1. Executive Summary

Prism is a lightweight, high-performance TCP reverse proxy designed primarily for the Minecraft protocol. It multiplexes incoming traffic from a single port to multiple backend servers based on a hostname found in the connection's initial bytes.

While Minecraft handshake parsing is the primary use-case, Prism also supports extracting hostnames from standard TLS SNI so the same routing stack can be reused for other TCP frontends.

The system is designed with a **test-first architecture**, ensuring that core logic (protocol parsing, routing, traffic shaping) can be verified via unit tests without requiring active network sockets or running Minecraft server instances.

---

## 2. Architecture Overview

The system follows a **Layered Architecture**. Each layer communicates with the next via defined interfaces, allowing for easy mocking and stubbing during testing.

### High-Level Data Flow

1. **Transport Layer**: Accepts raw TCP connections.
2. **Header Parsing Layer**: Reads/peeks the initial bytes to extract a routing hostname.
3. **Routing Layer**: Resolves the destination address based on the extracted hostname.
4. **Proxy Layer**: Establishes the upstream connection and pipes data between client and server.

---

## 3. Core Component Design

### 3.1. `Listener` (Transport Layer)

* **Responsibility**: Wraps the standard `net.Listener`. It accepts connections and spawns a `SessionHandler` for each.
* **Testability**:
* The `Listener` accepts a `ConnectionHandler` interface.
* This allows integration tests to simulate a flood of connections without spawning real goroutines for the entire logic stack.



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


* **Testability**:
  * Defined as an interface `UpstreamResolver`.
  * Tests can inject a `MockRouter` that returns deterministic results, decoupling routing logic from configuration file parsing.



### 3.5. `UpstreamDialer` (Network Abstraction)

* **Responsibility**: Establishes the connection to the backend Minecraft server.
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
* `POST /reload`: Triggers a configuration reload and atomically swaps the snapshot used for new connections.



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
* **Zero-downtime config reload**:
  * Prism can reload configuration without stopping the TCP listener.
  * Existing sessions continue with the configuration snapshot they started with.
  * New connections use the latest configuration snapshot.
  * Not all fields can be reloaded safely (e.g., listen address/admin address require a restart); reloadable fields include routes, timeouts, dial timeouts, buffer sizing and header parser selection.

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

## 8. WASM Routing Parser ABI (v1)

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


## 7. Directory Structure (Proposed)

```text
/cmd/prism/        # Entry point (main.go)
/internal/
    /protocol/     # Routing header parsers (MC handshake, TLS SNI, WASM)
    /proxy/        # ProxyBridge, SessionHandler
    /router/       # Router, Config loading
    /server/       # TCPServer/Listener wrapper
    /telemetry/    # MetricsCollector, AdminServer
/pkg/              # Publicly usable libraries (if any)
    /mcproto/      # Low-level MC varint decoding (could be reused)
```
