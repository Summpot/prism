---
title: Admin API
sidebar_position: 3
---

Prism exposes an optional admin HTTP server controlled by `admin_addr`.

- Set `admin_addr = ":8080"` (default) to enable.
- Set `admin_addr = ""` to disable.

All endpoints are rooted at the configured address.

## Endpoints

### `GET /health`

Health check.

Response:

```json
{"ok":true}
```

### `GET /metrics`

Prometheus text format metrics.

- Content-Type: `text/plain; version=0.0.4`

### `GET /conns`

JSON snapshot of active connections.

This is intended for debugging/observability.

### `GET /tunnel/services`

JSON snapshot of registered tunnel services (empty list if tunnel is disabled).

### `GET /config`

Returns the resolved config file path:

```json
{"path":"/etc/prism/prism.toml"}
```

### `POST /reload`

Trigger an on-demand config reload.

Response:

```json
{"seq":123}
```

## Examples

```text
curl -fsS http://127.0.0.1:8080/health
curl -fsS http://127.0.0.1:8080/metrics
curl -fsS http://127.0.0.1:8080/conns
curl -fsS http://127.0.0.1:8080/tunnel/services
curl -fsS -X POST http://127.0.0.1:8080/reload
curl -fsS http://127.0.0.1:8080/config
```
