---
title: CLI 与环境变量
sidebar_position: 2
---

## CLI 参数

与路径相关的常用参数：

- `--config <path>`：配置文件路径
- `--workdir <path>`：workdir（运行时状态目录）
- `--routing-parser-dir <path>`：路由解析器目录

完整参数请运行 `prism --help`。

## 环境变量

与 CLI 对应的环境变量：

- `PRISM_CONFIG`：配置文件路径
- `PRISM_WORKDIR`：workdir
- `PRISM_ROUTING_PARSER_DIR`：路由解析器目录

## 默认值（摘要）

- 在当前目录自动发现：`prism.toml` → `prism.yaml` → `prism.yml`
- 配置默认回退：
  - Linux：`/etc/prism/prism.toml`
  - 其他系统：`${ProjectConfigDir}/prism.toml`
- workdir 默认：
  - Linux：`/var/lib/prism`
  - 其他系统：`${ProjectDataDir}`
- parsers 默认目录：
  - `<config_dir>/parsers`（Linux 默认：`/etc/prism/parsers`）

## 仅容器镜像使用的变量

官方镜像额外支持：

- `PRISM_UID` / `PRISM_GID`

用于 entrypoint 在 bind-mount 场景下选择运行 UID/GID。
