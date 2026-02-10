---
title: 路由解析器（WASM）
sidebar_position: 2
---

Prism 通过 **路由解析器** 从 TCP 连接的最初字节中提取用于路由的主机名。

路由解析器是 `.wat` 文件（WebAssembly 文本格式）。Prism 会从磁盘目录加载这些模块并执行它们来得到 hostname。

## 内置解析器

Prism 内置两个解析器：

- `minecraft_handshake`
- `tls_sni`

启动时，Prism 会把内置 `.wat` 模块 **写入** 到路由解析器目录（如果文件缺失）。

## 解析器放在哪

解析器目录可通过以下方式配置：

- CLI：`--routing-parser-dir /path/to/parsers`
- 环境变量：`PRISM_ROUTING_PARSER_DIR=/path/to/parsers`
- 默认值：`<config_dir>/parsers`（Linux 默认：`/etc/prism/parsers`）

解析器名字会映射到文件：

- `parsers = ["minecraft_handshake"]` → `<routing_parser_dir>/minecraft_handshake.wat`

## 相对路径

如果 `--routing-parser-dir` 是相对路径（例如 `parsers`），Prism 会以 **配置文件所在目录** 为基准解析。

## 安全与格式说明

- Prism 有意 **不加载原始 `.wasm` 二进制** 作为路由解析器。
- 路由解析器会处理来自不可信客户端的输入（TCP 流头部），建议保持实现小而健壮。

## 排障

### 找不到解析器文件

- 确认 `routes[*].parsers` 中的名字与文件名一致（不包含 `.wat`）。
- 确认该文件确实存在于解析器目录中。
- 将 `logging.level` 调为 `debug` 查看详细日志。
