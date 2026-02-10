---
title: Configuration
sidebar_position: 1
---

Prism configuration is available in TOML (`prism.toml`) and YAML (`prism.yaml` / `prism.yml`).

A JSON Schema is available for editor validation/completions:

- `prism.schema.json` (in the repo root)

## Top-level fields

### `listeners` (array)

Public-facing proxy listeners.

Each listener has:

- `listen_addr` (required): address to bind (example `:25565`)
- `protocol`: `tcp` (default) or `udp`
- `upstream`:
  - TCP: empty/omitted means hostname-routing mode; non-empty means fixed forward
  - UDP: required (always fixed forward)

### `routes` (array)

Ordered hostname routes (first match wins).

Each route supports:

- `host` / `hosts`: host pattern(s), supports `*` and `?` wildcards
- `upstream` / `upstreams` (aliases: `backend` / `backends`)
- `strategy`: `sequential` (default), `random`, `round-robin`
- `parsers`: routing parser chain (defaults to `[minecraft_handshake, tls_sni]`)
- `cache_ping_ttl`: cache Minecraft status responses (`60s`, `500ms`, `-1`)

### `admin_addr` (string)

Admin HTTP server listen address.

- Example: `:8080`
- Empty string disables the admin server.

### `logging` (object)

- `level`: `debug` | `info` | `warn` | `error`
- `format`: `json` | `text`
- `output`: `stderr` | `stdout` | `discard` | file path
- `add_source`: include source file/line

### `reload` (object)

Automatic reload watching (file-based provider only):

- `enabled` (default `true`)
- `poll_interval_ms` (default `1000`)

### `timeouts` (object)

- `handshake_timeout_ms` (default `3000`)
- `idle_timeout_ms` (default `0`, disabled)

### `proxy_protocol_v2` (bool)

Whether to inject HAProxy PROXY protocol v2 headers on TCP upstream connections.

### `buffer_size` (int)

Buffer size in bytes used for proxying.

- `0` means “use the default”.

### `upstream_dial_timeout_ms` (int)

Dial timeout for upstream connections in milliseconds.

- `0` means “use the default”.

### `max_header_bytes` (int)

Maximum number of bytes to peek/read for routing (handshake/SNI/etc).

- `0` means “use the default”.

### `tunnel` (object)

Reverse-connection mode (client → server).

- `auth_token`: optional shared secret for client registration
- `auto_listen_services`: whether the server auto-opens listeners for services with `remote_addr`
- `endpoints`: server endpoints to accept tunnel clients
- `client`: optional tunnel client role
- `services`: services registered by a client

For a complete example, see `prism.example.toml` / `prism.example.yaml` in the repo.

## Full schema

For exact field types, defaults, and validation rules, consult:

- `prism.schema.json`
