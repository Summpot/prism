---
title: Config & runtime paths
sidebar_position: 2
---

## Config discovery

Prism can find a config file automatically, or you can specify one explicitly.

### Explicit config

- CLI flag: `--config /path/to/prism.toml`
- Environment: `PRISM_CONFIG=/path/to/prism.toml`

### Auto-discovery

From the current working directory, Prism checks (in order):

1. `prism.toml`
2. `prism.yaml`
3. `prism.yml`

### Fallback default path

If no config is provided and no file is discovered, Prism falls back to:

- Linux: `/etc/prism/prism.toml`
- Other OSes: `${ProjectConfigDir}/prism.toml` (derived from Rust’s `directories::ProjectDirs`)

If the resolved config path does **not** exist, Prism will create a runnable default config file at that path and continue starting.

## Work directory

Prism uses a *work directory* for runtime state.

- CLI flag: `--workdir /path/to/workdir`
- Environment: `PRISM_WORKDIR=/path/to/workdir`
- Default:
  - Linux: `/var/lib/prism`
  - Other OSes: per-user data dir (from `directories::ProjectDirs`)

## Routing parsers directory

Routing parsers are `.wat` files (WebAssembly text format) used to extract a hostname from the first bytes of a TCP stream.

- CLI flag: `--routing-parser-dir /path/to/parsers`
- Environment: `PRISM_ROUTING_PARSER_DIR=/path/to/parsers`
- Default: `<config_dir>/parsers` (Linux default: `/etc/prism/parsers`)

### Relative paths

If you pass a **relative** `--routing-parser-dir` (or env var), it is resolved relative to the **config directory** (the directory containing the resolved config file).

This makes it easy to keep everything together:

```text
/etc/prism/prism.toml
/etc/prism/parsers/
```
