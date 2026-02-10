---
title: 隧道模式
sidebar_position: 3
---

Prism 支持类似 frp 的隧道模式：内网机器（client）主动连出到公网机器（server），注册服务（service），然后公网侧根据路由把流量转发到这些服务。

## 基本概念

- **隧道 server**：具备公网可达性的 Prism。
- **隧道 client**：运行在内网/私网后端附近的 Prism。
- **service**：例如 `home-mc`，映射到本机地址 `local_addr`。

在 routes 中通过以下方式引用：

- `tunnel:<service>`

## server 配置

在公网机器上：

1. 配置 `tunnel.endpoints` 以接收隧道 client。
2. 配置常规的 `listeners` 与 `routes`。

示例：

```toml
[[listeners]]
listen_addr = ":25565"
protocol = "tcp"

[tunnel]
auth_token = "" # 可选：要求共享密钥

[[tunnel.endpoints]]
listen_addr = ":7000"
transport = "tcp"

[[routes]]
host = "home.example.com"
upstream = "tunnel:home-mc"
```

## client 配置

在内网机器上：

1. 配置 `tunnel.client` 连接公网 server。
2. 配置 `tunnel.services` 注册服务。

示例：

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

## 端口暴露（remote_addr）

如果你希望像 frp 一样在公网侧自动开放端口，可给服务设置 `remote_addr`：

```toml
[[tunnel.services]]
name = "demo"
local_addr = "127.0.0.1:25565"
remote_addr = ":25570"
```

注意：

- 如果 `route_only = true`，就必须 **不要** 设置 `remote_addr`。
- server 侧的 `tunnel.auto_listen_services` 控制是否允许自动开放端口。

## 多个 client 同名服务

如果多个隧道 client 注册了同一个服务名，Prism 会选择 **第一个** 仍然活跃的注册者作为 `tunnel:<service>` 的路由目标。

后续同名注册不会覆盖路由（但仍可能通过 `remote_addr` 方式暴露端口）。

## 传输协议

隧道 endpoints 与 client 支持：

- `tcp`
- `udp`
- `quic`

QUIC 需要 TLS；当 QUIC 的证书/私钥路径为空时，Prism 可以在启动时生成自签证书用于便捷部署。
