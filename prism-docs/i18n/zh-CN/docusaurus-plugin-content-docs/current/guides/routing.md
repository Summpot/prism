---
title: 路由
sidebar_position: 1
---

Prism 通过解析 TCP 流的最初字节来提取 **hostname**（Minecraft 握手、TLS SNI 或自定义路由解析器），并据此将连接转发到不同上游。

## 监听器两种模式

`listeners` 中的每一项可以运行在两种模式之一：

- **按域名路由（TCP）**：`protocol = "tcp"` 且省略 `upstream`。
- **固定转发**：
  - TCP：`protocol = "tcp"` 且设置 `upstream`。
  - UDP：`protocol = "udp"` 且必须设置 `upstream`。

示例：

```toml
[[listeners]]
listen_addr = ":25565"
protocol = "tcp"   # 按域名路由

[[listeners]]
listen_addr = ":19132"
protocol = "udp"
upstream = "127.0.0.1:19132"  # 固定转发
```

## 路由匹配规则

`routes` 是一个 **有序列表**：

- 按顺序检查。
- **第一个匹配** 的路由生效。

建议把更具体的域名规则放在更前面。

### host 通配符

`host`（或 `hosts`）支持类似 glob 的通配符（不区分大小写）：

- `*` 匹配任意字符串（会被 **捕获** 为分组）
- `?` 匹配任意单字符（会被 **捕获** 为分组）

示例：

- `play.example.com`
- `*.example.com`
- `*.labs.??.example.com`

### 在 upstream 中引用捕获组

如果 host 模式中包含通配符，upstream 字符串可以用 `$1`、`$2`… 引用捕获组。

```toml
[[routes]]
host = "*.example.com"
upstream = "$1.internal.example.com:25565"
```

## 上游（upstreams）

一个路由可以使用：

- `upstream`（单个）
- `upstreams`（多个）

也提供兼容别名：

- `backend` / `backends`

### 负载均衡策略

当配置多个上游时，`strategy` 控制探测顺序：

- `sequential`（默认）
- `random`
- `round-robin`

如果拨号失败，Prism 会回退尝试下一个上游。

### 端口选择

上游既可以是 `host:port`，也可以是 `tunnel:<service>`。

如果 upstream 省略端口（例如 `backend.example.com`），Prism 会：

1. 优先使用 Minecraft 握手中携带的端口（如果可用）
2. 否则使用命中的 listener 端口（常见为 `25565`）

## Minecraft 状态（ping）缓存

设置 `cache_ping_ttl` 后，Prism 可以缓存 Minecraft status 响应。

- 使用类似 `60s`、`500ms`、`2m` 的时间字符串。
- 使用 `-1` 禁用缓存。

```toml
[[routes]]
host = "play.example.com"
upstream = "127.0.0.1:25566"
cache_ping_ttl = "60s"
```

## 路由解析器

每个路由可以指定解析器链：

```toml
[[routes]]
host = "play.example.com"
upstream = "127.0.0.1:25566"
parsers = ["minecraft_handshake", "tls_sni"]
```

如果省略 `parsers`，Prism 默认使用：

- `["minecraft_handshake", "tls_sni"]`

解析器目录与加载规则详见：**Guides → Routing parsers**。
