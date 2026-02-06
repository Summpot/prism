# Prism

Prism 是一个轻量、高性能的 Minecraft 协议 TCP 反向代理（L4），根据连接握手中的主机名（Minecraft handshake / TLS SNI / WASM）将流量转发到不同上游。

- **数据面**：TCP 监听（默认 `:25565`）
- **管理面**：HTTP 管理端口（默认 `:8080`）

项目的架构说明见 `DESIGN.md`。

## 快速开始

Prism 支持 `.toml` / `.yaml` / `.yml` / `.json` 配置文件。

- 显式指定：`prism -config /path/to/prism.json`
- 自动发现（从当前工作目录按顺序）：`prism.toml` > `prism.yaml` > `prism.yml` > `prism.json`

仓库中包含示例配置：

- `config.example.json`
- `prism.example.toml`
- `prism.example.yaml`

### 本地运行

1. 复制一份配置文件到运行目录（例如 `prism.json`）
2. 启动：

- Windows (PowerShell)：`./prism.exe -config prism.json`
- Linux/macOS：`./prism -config prism.json`

### 路由示例

在配置中用 `routes` 定义 hostname 到上游地址的映射：

- 精确匹配：`play.example.com` → `127.0.0.1:25566`
- 通配符：`*.labs.example.com` → `127.0.0.1:25567`

可直接参考 `config.example.json`。

## Docker

本仓库提供 `Dockerfile`，并在 GitHub Actions 中构建并推送镜像到 GHCR：

- `ghcr.io/<owner>/<repo>`（例如 `ghcr.io/Summpot/prism`）

容器内默认工作目录是 `/config`，因此**把配置文件挂载到 `/config/prism.json`（或 prism.toml/yaml）后，无需额外参数即可自动发现**。

### 运行（Linux/macOS）

- 代理端口：`25565/tcp`
- 管理端口：`8080/tcp`

示例：

- 将本地 `prism.json` 挂载到容器内 `/config/prism.json`
- 映射端口并运行

```text
docker run --rm \
  -p 25565:25565 \
  -p 8080:8080 \
  -v "$PWD/prism.json:/config/prism.json:ro" \
  ghcr.io/Summpot/prism:latest
```

### 运行（Windows PowerShell）

```text
docker run --rm `
  -p 25565:25565 `
  -p 8080:8080 `
  -v "${PWD}\prism.json:/config/prism.json:ro" `
  ghcr.io/Summpot/prism:latest
```

> 如果你把配置文件命名成其他名字/路径，可以用 `-config` 显式指定，例如：
> `prism -config /config/myconfig.toml`

## 管理接口（Admin）

默认监听地址由 `admin_addr` 控制（默认 `:8080`）。

- `GET /health`：健康检查（非 200 表示服务不可用）
- `GET /metrics`：JSON 指标快照
- `GET /conns`：当前连接快照
- `GET /logs?limit=200`：最近日志行（需在 `logging.admin_buffer.enabled=true` 时启用）
- `POST /reload`：触发一次配置重载（需开启 reload 功能）

## 构建

需要 Go（版本以 `go.mod` 为准）。

- 编译：`go build ./cmd/prism`
- 测试：`go test ./...`

## GitHub Actions

仓库内置工作流：`.github/workflows/build-release.yml`

- PR / push：运行 `go test ./...`，并构建多平台二进制（作为 artifact）
- tag（建议形如 `v1.2.3`）：
  - 生成 GitHub Release 并附带多平台二进制压缩包与 `checksums.txt`
  - 构建并推送多架构 Docker 镜像（`linux/amd64` + `linux/arm64`）到 GHCR
