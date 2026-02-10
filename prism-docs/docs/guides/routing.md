---
title: Routing
sidebar_position: 1
---

Prism routes incoming TCP connections by extracting a **hostname** from the first bytes of the stream (Minecraft handshake, TLS SNI, or a custom routing parser).

## Listener modes

Each entry in `listeners` can run in one of two modes:

- **Hostname-routing mode (TCP)**: set `protocol = "tcp"` and omit `upstream`.
- **Fixed-forward mode**:
  - TCP: set `protocol = "tcp"` and set `upstream`.
  - UDP: set `protocol = "udp"` and set `upstream` (required).

Example:

```toml
[[listeners]]
listen_addr = ":25565"
protocol = "tcp"   # hostname routing

[[listeners]]
listen_addr = ":19132"
protocol = "udp"
upstream = "127.0.0.1:19132"  # fixed forward
```

## Route matching

`routes` is an **ordered list**.

- Routes are checked in order.
- The **first match wins**.

Put more specific host patterns earlier.

### Host patterns

`host` (or `hosts`) supports glob-like wildcards (case-insensitive):

- `*` matches any string (**captured** as a group)
- `?` matches any single character (**captured** as a group)

Examples:

- `play.example.com`
- `*.example.com`
- `*.labs.??.example.com`

### Capture groups in upstreams

If the host pattern contains wildcards, upstream strings may reference captured groups as `$1`, `$2`, ...

Example:

```toml
[[routes]]
host = "*.example.com"
upstream = "$1.internal.example.com:25565"
```

## Upstreams

A route may specify:

- `upstream` (single)
- `upstreams` (multiple)

Aliases also exist for compatibility:

- `backend` / `backends`

### Load balancing strategy

When multiple upstreams are configured, `strategy` controls the probe order:

- `sequential` (default)
- `random`
- `round-robin`

If dialing the chosen upstream fails, Prism tries the next one.

### Port selection

Upstreams can be `host:port` or `tunnel:<service>`.

If an upstream omits the port (for example `backend.example.com`), Prism will:

1. prefer the port from the Minecraft handshake (when available)
2. otherwise fall back to the port from the matched listener (commonly `25565`)

## Caching Minecraft status (ping)

If you set `cache_ping_ttl`, Prism can cache Minecraft status responses.

- Use a human-time string like `60s`, `500ms`, `2m`.
- Use `-1` to disable caching.

```toml
[[routes]]
host = "play.example.com"
upstream = "127.0.0.1:25566"
cache_ping_ttl = "60s"
```

## Routing parsers

Each route can specify a parser chain:

```toml
[[routes]]
host = "play.example.com"
upstream = "127.0.0.1:25566"
parsers = ["minecraft_handshake", "tls_sni"]
```

If `parsers` is omitted, Prism defaults to:

- `["minecraft_handshake", "tls_sni"]`

For details on where parsers live and how they are loaded, see **Guides → Routing parsers**.
