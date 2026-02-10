---
title: Docker
sidebar_position: 4
---

Prism 提供发布到 GHCR 的容器镜像。

## 容器内默认路径

- 配置目录 / 工作目录：`/etc/prism`
- 默认配置路径：`/etc/prism/prism.toml`
- 默认 workdir（运行时状态）：`/var/lib/prism`
- 默认路由解析器目录：`/etc/prism/parsers`

## 常见运行方式

### 挂载单个配置文件（只读）

```text
docker run --rm \
  -p 25565:25565 \
  -p 8080:8080 \
  -v "$PWD/prism.toml:/etc/prism/prism.toml:ro" \
  ghcr.io/Summpot/prism:latest
```

### 挂载配置目录（读写）

这样 Prism 可以在首次启动时自动生成 `/etc/prism/prism.toml`。

```text
docker run --rm \
  -p 25565:25565 \
  -p 8080:8080 \
  -v "$PWD/config:/etc/prism" \
  ghcr.io/Summpot/prism:latest
```

### 持久化 workdir（运行时状态）

```text
docker run --rm \
  -p 25565:25565 \
  -p 8080:8080 \
  -v "$PWD/config:/etc/prism" \
  -v "$PWD/workdir:/var/lib/prism" \
  ghcr.io/Summpot/prism:latest
```

## bind-mount 权限

在 bind-mount 场景下，不同系统与 Docker 实现会导致目录所有权/权限表现不一致。

Prism 镜像的 entrypoint 会尽量：

1. 创建所需目录
2. 尽可能以非 root 用户运行
3. 若确实无法写入则回退为 root 运行，以避免直接启动失败

如果你希望显式控制，可以设置：

- `PRISM_UID` / `PRISM_GID`（仅容器镜像使用）

或直接用：

- `--user <uid>:<gid>`

如果仍无法写入，请确保宿主机目录对选定 UID/GID 可写。
