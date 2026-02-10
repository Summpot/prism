---
title: 管理 API
sidebar_position: 3
---

Prism 提供可选的管理 HTTP 服务，由 `admin_addr` 控制。

- 设置 `admin_addr = ":8080"`（默认）启用。
- 设置 `admin_addr = ""` 禁用。

以下接口均以配置的监听地址为根路径。

## 接口列表

### `GET /health`

健康检查。

返回：

```json
{"ok":true}
```

### `GET /metrics`

Prometheus 文本格式指标。

- Content-Type：`text/plain; version=0.0.4`

### `GET /conns`

返回当前活跃连接的 JSON 快照（用于调试/观测）。

### `GET /tunnel/services`

返回已注册隧道服务的 JSON 快照（若未启用 tunnel，则返回空数组）。

### `GET /config`

返回当前解析到的配置文件路径：

```json
{"path":"/etc/prism/prism.toml"}
```

### `POST /reload`

触发一次按需重载。

返回：

```json
{"seq":123}
```

## 示例

```text
curl -fsS http://127.0.0.1:8080/health
curl -fsS http://127.0.0.1:8080/metrics
curl -fsS http://127.0.0.1:8080/conns
curl -fsS http://127.0.0.1:8080/tunnel/services
curl -fsS -X POST http://127.0.0.1:8080/reload
curl -fsS http://127.0.0.1:8080/config
```
