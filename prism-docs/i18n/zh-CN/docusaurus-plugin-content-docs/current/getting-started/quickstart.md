---
title: 快速开始
sidebar_position: 1
---

## 运行 Prism

Prism 是单个可执行文件，读取 TOML/YAML 配置后启动：

- 一个或多个公网代理监听（常见为 `:25565/tcp`）
- 可选的管理 HTTP 服务（常见为 `:8080`）

### 方式 A：Docker

使用官方镜像时，容器内默认工作目录是 `/etc/prism`。

- 如果你把配置文件挂载到 `/etc/prism/prism.toml`（或 `prism.yaml` / `prism.yml`），Prism 会自动发现它。

最小示例：

```text
docker run --rm \
  -p 25565:25565 \
  -p 8080:8080 \
  -v "$PWD/prism.toml:/etc/prism/prism.toml:ro" \
  ghcr.io/Summpot/prism:latest
```

### 方式 B：本地运行

从源码构建：

```text
cargo build -p prism
```

通过显式配置路径运行：

```text
./target/debug/prism --config /path/to/prism.toml
```

## 最小配置

一个最简单的“按域名路由”配置如下：

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

## 验证

- Prism 日志应显示代理监听地址与（如启用）管理监听地址。
- 健康检查：

```text
curl -fsS http://127.0.0.1:8080/health
```

如果你需要 Prometheus 指标：

```text
curl -fsS http://127.0.0.1:8080/metrics
```

## 下一步

- **Guides → Routing**：通配符匹配、负载均衡、端口选择。
- **Guides → Tunnel mode**：frp 风格的反向隧道。
- **Reference → Configuration**：完整配置字段参考（含默认值）。
