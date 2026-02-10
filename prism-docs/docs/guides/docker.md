---
title: Docker
sidebar_position: 4
---

Prism ships a container image published to GHCR.

## Default paths inside the container

- Config directory / working directory: `/etc/prism`
- Default config path: `/etc/prism/prism.toml`
- Default workdir (runtime state): `/var/lib/prism`
- Default routing parsers dir: `/etc/prism/parsers`

## Common run patterns

### Mount a single config file (read-only)

```text
docker run --rm \
  -p 25565:25565 \
  -p 8080:8080 \
  -v "$PWD/prism.toml:/etc/prism/prism.toml:ro" \
  ghcr.io/Summpot/prism:latest
```

### Mount a config directory (read-write)

This lets Prism create `/etc/prism/prism.toml` on first start if it doesn’t exist.

```text
docker run --rm \
  -p 25565:25565 \
  -p 8080:8080 \
  -v "$PWD/config:/etc/prism" \
  ghcr.io/Summpot/prism:latest
```

### Persist runtime state (workdir)

```text
docker run --rm \
  -p 25565:25565 \
  -p 8080:8080 \
  -v "$PWD/config:/etc/prism" \
  -v "$PWD/workdir:/var/lib/prism" \
  ghcr.io/Summpot/prism:latest
```

## Permissions on bind mounts

On bind mounts, ownership/permissions can vary by host OS and Docker implementation.

Prism’s image uses an entrypoint that tries to:

1. create required directories
2. run Prism as a non-root user when possible
3. fall back to running as root only when necessary to avoid startup failure

If you want explicit control, you can set:

- `PRISM_UID` and `PRISM_GID` (container-only)

or run the container with:

- `--user <uid>:<gid>`

If Prism still can’t write to your mounted directory, ensure the host directory is writable by the chosen UID/GID.
