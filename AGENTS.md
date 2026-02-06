# AGENTS.md

This document defines how automated agents (and humans operating like them) should work in this repository.

Prism is a lightweight, high-performance TCP reverse proxy for the Minecraft protocol. The intended architecture is documented in `DESIGN.md`.

## Non‑negotiables

1. **Design consistency (required)**
   - When the user makes a request, **check whether the request matches `DESIGN.md`**.
   - If it does **not** match, **update `DESIGN.md` in the same change** (or immediately before) so the design and implementation remain consistent.
   - Do not implement a behavior that contradicts the design without also updating the design.

2. **Keep the project test-first**
   - Add/adjust tests for behavior changes.
   - Ensure `go test ./...` passes before finishing.

3. **Prefer minimal, reviewable diffs**
   - Make small, incremental changes.
   - Avoid unrelated refactors/renames and avoid reformatting files beyond what `gofmt` changes in touched code.

## Architecture & boundaries (match existing code)

- Keep the layered shape from `DESIGN.md`:
  - Transport: `internal/server` (accept connections)
  - Protocol: `internal/protocol` (handshake decoding)
  - Routing: `internal/router` (hostname → upstream)
  - Proxy: `internal/proxy` (session orchestration + bridging)
  - Telemetry/Admin: `internal/telemetry`
  - Reusable low-level protocol utilities live in `pkg/mcproto`

- **Dependency direction:** `cmd/prism` wires components together; core packages should not import `cmd/...`.

- **Pure logic stays pure:**
  - Prefer `io.Reader`/`io.Writer`/`io.ReadWriter` in core logic.
  - Avoid coupling business logic directly to `net.Conn` unless necessary.

- **Interfaces for testability:**
  - Follow existing patterns such as `protocol.HandshakeDecoder`, `router.UpstreamResolver`, `proxy.Dialer`, and the bridge/buffer pool abstractions.

## Concurrency, timeouts, cancellation

- Thread cancellation should flow from `context.Context`.
- Avoid goroutine leaks:
  - Any goroutine started for a session should terminate on context cancellation.
  - Prefer bounded channels and deterministic shutdown paths.
- When applying timeouts, follow the existing `config.Timeouts` behavior.

## Error handling & logging

- Prefer returning errors to the caller when possible.
- When dropping an error intentionally (e.g., for security/noise reasons), leave a short comment explaining why.
- Use clear, scoped error messages (package prefix like `protocol:` is already used).

### Structured logging conventions

- Use `log/slog` (structured logs). Avoid adding new `log.Printf` style logging.
- Prefer stable, queryable keys over interpolated strings. Common keys:
  - `sid` (session id)
  - `client` (remote addr)
  - `host` (routed hostname)
  - `upstream` (upstream addr)
  - `err` (error)
- Avoid hot-path logging; Prism is performance-oriented:
  - No per-connection logs at `info` by default.
  - Per-connection details should be `debug` and guarded with `logger.Enabled(ctx, slog.LevelDebug)` when building non-trivial attributes.
- Do not log raw captured handshake bytes at `info`/`warn` (can be noisy and potentially sensitive). If needed for deep debugging, log only lengths/counts, or put raw data behind `debug` with explicit justification.

## Performance & allocations

- Preserve buffer pooling behavior in `internal/proxy`.
- Avoid unnecessary allocations on the connection hot path.
- Any new feature that adds per-connection overhead should include a short justification in code comments and (when appropriate) a test.

## Configuration changes

- Configuration is JSON (see `config.example.json` / `config.json` and `internal/config`).
- If you change the config schema:
  - Update `internal/config` structs and parsing.
  - Update **both** `config.example.json` and `config.json` (or clearly justify why not).
  - Consider backward compatibility and defaults.

## Testing conventions (follow existing tests)

- Prefer standard Go tests (`testing` package).
- Favor table-driven tests where there are multiple cases.
- Use `net.Pipe()` for integration-style tests (already used in `internal/proxy/session_integration_test.go`).
- Keep tests deterministic and avoid timing flakiness (use generous timeouts only when needed).

## Formatting & style

- Use `gofmt` formatting.
- Keep imports clean and consistent with current files.
- Keep names and exported APIs small and purposeful; prefer package-private helpers unless there is a clear reuse need.
