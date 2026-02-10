---
title: 配置参考
sidebar_position: 1
---

Prism 支持 TOML（`prism.toml`）与 YAML（`prism.yaml` / `prism.yml`）配置。

仓库根目录提供 JSON Schema，用于编辑器校验与补全：

- `prism.schema.json`

## 顶层字段

### `listeners`（数组）

公网代理监听器。

每个 listener 包含：

- `listen_addr`（必填）：绑定地址（例如 `:25565`）
- `protocol`：`tcp`（默认）或 `udp`
- `upstream`：
  - TCP：为空/省略表示按域名路由；非空表示固定转发
  - UDP：必填（始终固定转发）

### `routes`（数组）

按顺序匹配的域名路由（第一个匹配生效）。

每个 route 支持：

- `host` / `hosts`：域名模式，支持 `*` / `?` 通配
- `upstream` / `upstreams`（别名：`backend` / `backends`）
- `strategy`：`sequential`（默认）、`random`、`round-robin`
- `parsers`：解析器链（默认 `[minecraft_handshake, tls_sni]`）
- `cache_ping_ttl`：缓存 Minecraft status（`60s`、`500ms`、`-1`）

### `admin_addr`（字符串）

管理 HTTP 服务监听地址。

- 例如 `:8080`
- 设为空字符串会禁用管理服务。

### `logging`（对象）

- `level`：`debug` | `info` | `warn` | `error`
- `format`：`json` | `text`
- `output`：`stderr` | `stdout` | `discard` | 文件路径
- `add_source`：是否包含源码文件/行号

### `reload`（对象）

自动重载监控（仅文件型 provider）：

- `enabled`（默认 `true`）
- `poll_interval_ms`（默认 `1000`）

### `timeouts`（对象）

- `handshake_timeout_ms`（默认 `3000`）
- `idle_timeout_ms`（默认 `0`，禁用）

### `proxy_protocol_v2`（布尔）

是否在 TCP 上游连接中注入 HAProxy PROXY protocol v2 头部，用于在后端保留真实客户端地址。

### `buffer_size`（整数）

代理转发使用的 buffer 大小（字节）。

- `0` 表示使用默认值。

### `upstream_dial_timeout_ms`（整数）

连接上游的拨号超时时间（毫秒）。

- `0` 表示使用默认值。

### `max_header_bytes`（整数）

用于路由判断时最多读取/窥探的字节数（握手/SNI 等）。

- `0` 表示使用默认值。

### `tunnel`（对象）

反向连接隧道模式（client → server）。

- `auth_token`：可选共享密钥
- `auto_listen_services`：是否允许服务通过 `remote_addr` 请求自动开放端口
- `endpoints`：server 监听端点
- `client`：可选 client 角色
- `services`：client 注册的服务

完整示例可参考仓库中的 `prism.example.toml` / `prism.example.yaml`。

## 完整 schema

如需精确字段类型、默认值与校验规则，请以 `prism.schema.json` 为准。
