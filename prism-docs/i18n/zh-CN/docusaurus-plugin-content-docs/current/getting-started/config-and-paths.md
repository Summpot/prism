---
title: 配置文件与运行时路径
sidebar_position: 2
---

## 配置文件发现规则

Prism 可以自动发现配置文件，也可以显式指定。

### 显式指定

- CLI：`--config /path/to/prism.toml`
- 环境变量：`PRISM_CONFIG=/path/to/prism.toml`

### 自动发现

在当前工作目录下，Prism 按以下顺序查找：

1. `prism.toml`
2. `prism.yaml`
3. `prism.yml`

### 默认回退路径

若没有提供配置且未发现文件，Prism 会回退到：

- Linux：`/etc/prism/prism.toml`
- 其他系统：`${ProjectConfigDir}/prism.toml`（由 Rust 的 `directories::ProjectDirs` 推导）

如果最终解析出的配置路径 **不存在**，Prism 会在该路径生成一个可运行的默认配置并继续启动。

## 工作目录（workdir）

Prism 使用 *workdir* 保存运行时状态。

- CLI：`--workdir /path/to/workdir`
- 环境变量：`PRISM_WORKDIR=/path/to/workdir`
- 默认值：
  - Linux：`/var/lib/prism`
  - 其他系统：每用户数据目录（由 `directories::ProjectDirs` 推导）

## 路由解析器目录（parsers）

路由解析器是 `.wat` 文件（WebAssembly 文本格式），用于从 TCP 流的最初字节中提取主机名。

- CLI：`--routing-parser-dir /path/to/parsers`
- 环境变量：`PRISM_ROUTING_PARSER_DIR=/path/to/parsers`
- 默认值：`<config_dir>/parsers`（Linux 默认：`/etc/prism/parsers`）

### 相对路径

如果你传入的是 **相对路径**（例如 `parsers`），Prism 会以 **配置文件所在目录** 为基准进行解析。

这样更方便把配置与解析器放在同一目录：

```text
/etc/prism/prism.toml
/etc/prism/parsers/
```
