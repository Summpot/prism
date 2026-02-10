---
sidebar_position: 1
---

# Prism

Prism 是一个轻量、高性能的 TCP 反向代理与隧道（类似 frp），主要面向 Minecraft 风格的“按域名路由”。

它会接收公网 TCP 连接（常见为 `:25565`），从连接的最初字节中提取目标主机名（Minecraft 握手 / TLS SNI / WASM 路由解析器），然后转发到选定的上游。

## 你可以在这里找到什么

- **使用教程 / 指南**：如何运行 Prism、配置路由、使用隧道模式。
- **配置参考**：所有配置字段的解释与示例。
- **API 参考**：健康检查、指标、连接信息与热重载等管理接口。

## 快速入口

- 从这里开始：**Getting started → Quickstart**
- 配置说明：**Reference → Configuration**
- 运维接口：**Reference → Admin API**

## 支持的配置格式

- TOML：`prism.toml`
- YAML：`prism.yaml` / `prism.yml`

Prism 可以从当前工作目录自动发现配置文件，也可以通过 `--config` / `PRISM_CONFIG` 显式指定。
