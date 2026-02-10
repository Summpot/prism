---
title: Quickstart
sidebar_position: 1
---

## Run Prism

Prism is a single binary that reads a TOML/YAML config file and starts:

- one or more public proxy listeners (commonly `:25565/tcp`)
- an optional admin HTTP server (commonly `:8080`)

### Option A: Docker

If you use the published image, Prism’s default working directory is `/etc/prism`.

- If you bind-mount a config file to `/etc/prism/prism.toml` (or `prism.yaml` / `prism.yml`), Prism will auto-discover it.

Minimal example:

```text
docker run --rm \
  -p 25565:25565 \
  -p 8080:8080 \
  -v "$PWD/prism.toml:/etc/prism/prism.toml:ro" \
  ghcr.io/Summpot/prism:latest
```

### Option B: Local binary

Build from source:

```text
cargo build -p prism
```

Run with an explicit config path:

```text
./target/debug/prism --config /path/to/prism.toml
```

## Minimal config

A small “hostname routing” config looks like this:

### TOML

```toml
admin_addr = ":8080"

[[listeners]]
listen_addr = ":25565"
protocol = "tcp"

[[routes]]
host = "play.example.com"
upstream = "127.0.0.1:25566"
```

### YAML

```yaml
admin_addr: ":8080"
listeners:
  - listen_addr: ":25565"
    protocol: "tcp"

routes:
  - host: "play.example.com"
    upstream: "127.0.0.1:25566"
```

## Verify

- Prism logs should show the proxy listener address and (if enabled) the admin listen address.
- Health check:

```text
curl -fsS http://127.0.0.1:8080/health
```

If you prefer Prometheus metrics:

```text
curl -fsS http://127.0.0.1:8080/metrics
```

## Next steps

- **Guides → Routing**: wildcard hosts, load balancing strategies, and port selection.
- **Guides → Tunnel mode**: run an frp-like reverse tunnel.
- **Reference → Configuration**: full field-by-field reference (with defaults).
