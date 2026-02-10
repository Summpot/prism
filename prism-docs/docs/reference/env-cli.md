---
title: CLI flags & environment variables
sidebar_position: 2
---

## CLI flags

Prism supports these path-related flags:

- `--config <path>`: config file path
- `--workdir <path>`: work directory for runtime state
- `--routing-parser-dir <path>`: routing parser directory

Run `prism --help` for the full CLI.

## Environment variables

Prism supports env vars matching the flags:

- `PRISM_CONFIG`: config file path
- `PRISM_WORKDIR`: work directory
- `PRISM_ROUTING_PARSER_DIR`: routing parser directory

## Defaults (summary)

- Config discovery from CWD: `prism.toml` → `prism.yaml` → `prism.yml`
- Fallback config path:
  - Linux: `/etc/prism/prism.toml`
  - other OSes: `${ProjectConfigDir}/prism.toml`
- Default workdir:
  - Linux: `/var/lib/prism`
  - other OSes: `${ProjectDataDir}`
- Default routing parsers dir:
  - `<config_dir>/parsers` (Linux default: `/etc/prism/parsers`)

## Container-only variables

The official container image also accepts:

- `PRISM_UID` / `PRISM_GID`

These are used by the entrypoint to pick a runtime UID/GID when running Prism on bind mounts.
