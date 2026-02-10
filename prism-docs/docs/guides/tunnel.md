---
title: Tunnel mode
sidebar_position: 3
---

Prism supports an frp-like tunnel mode where a private machine (client) creates an outbound connection to a public server, registers services, and the public server routes traffic to those services.

## Concepts

- **Tunnel server**: the Prism instance reachable from the public internet.
- **Tunnel client**: the Prism instance running near your private backend.
- **Service**: a name like `home-mc` mapped to a local address (`local_addr`).

In routes, tunnel services are referenced as:

- `tunnel:<service>`

## Server configuration

On the public server:

1. Configure `tunnel.endpoints` to accept tunnel clients.
2. Configure your normal `listeners` and `routes`.

Example:

```toml
[[listeners]]
listen_addr = ":25565"
protocol = "tcp"

[tunnel]
auth_token = "" # optionally require a shared secret

[[tunnel.endpoints]]
listen_addr = ":7000"
transport = "tcp"

[[routes]]
host = "home.example.com"
upstream = "tunnel:home-mc"
```

## Client configuration

On the private machine:

1. Configure `tunnel.client` to connect to the server.
2. Register one or more `tunnel.services`.

Example:

```toml
[tunnel]
auth_token = ""

[tunnel.client]
server_addr = "public.example.com:7000"
transport = "tcp"

[[tunnel.services]]
name = "home-mc"
proto = "tcp"
local_addr = "127.0.0.1:25565"
route_only = true
```

## Service exposure (remote_addr)

A service may also request that the server opens a listener automatically (frp-like) using `remote_addr`.

```toml
[[tunnel.services]]
name = "demo"
local_addr = "127.0.0.1:25565"
remote_addr = ":25570"
```

Notes:

- If `route_only = true`, you must **not** set `remote_addr`.
- On the server, `tunnel.auto_listen_services` controls whether Prism honors `remote_addr` requests.

## Multiple clients, same name

If multiple tunnel clients register the same service name, Prism keeps the **first** active registrant as the routing target for `tunnel:<service>`.

Later registrations with the same name do not override routing (they may still be exposed via `remote_addr`).

## Transports

Tunnel endpoints and clients support:

- `tcp`
- `udp`
- `quic`

QUIC requires TLS configuration; Prism can generate a self-signed certificate if you keep QUIC cert/key paths empty.
