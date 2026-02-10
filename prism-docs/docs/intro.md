---
sidebar_position: 1
---

# Prism

Prism is a lightweight, high-performance TCP reverse proxy and tunnel (frp-like) focused on Minecraft-style hostname routing.

It accepts a public TCP connection (commonly `:25565`), extracts the target hostname from the first bytes of the stream (Minecraft handshake / TLS SNI / WASM routing parsers), and forwards to the selected upstream.

## What you'll find here

- **Tutorials / guides**: how to run Prism, configure routing, and use tunnel mode.
- **Configuration reference**: every config field explained with examples.
- **Admin API reference**: endpoints for health, metrics, connections, and reload.

## Quick links

- Start here: **Getting started → Quickstart**
- Configure: **Reference → Configuration**
- Operate: **Reference → Admin API**

## Supported config formats

- TOML: `prism.toml`
- YAML: `prism.yaml` / `prism.yml`

Prism can auto-discover these from the current working directory, or you can pass a config path via `--config` / `PRISM_CONFIG`.
