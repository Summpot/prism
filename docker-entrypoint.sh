#!/bin/sh
set -eu

PRISM_BIN="/usr/local/bin/prism"

# Defaults (match Prism's runtime defaults on Linux).
DEFAULT_CONFIG_PATH="/etc/prism/prism.toml"
DEFAULT_WORKDIR="/var/lib/prism"

CONFIG_PATH="${PRISM_CONFIG:-}"
WORKDIR_PATH="${PRISM_WORKDIR:-}"

# Parse a minimal subset of CLI flags so we can prep dirs even when users pass flags
# instead of env vars. Supports both --flag value and --flag=value forms.
prev=""
for arg in "$@"; do
  case "$prev" in
    --config)
      CONFIG_PATH="$arg"
      prev=""
      continue
      ;;
    --workdir)
      WORKDIR_PATH="$arg"
      prev=""
      continue
      ;;
  esac

  case "$arg" in
    --config|--workdir)
      prev="$arg"
      ;;
    --config=*)
      CONFIG_PATH="${arg#*=}"
      ;;
    --workdir=*)
      WORKDIR_PATH="${arg#*=}"
      ;;
  esac
done

if [ -z "$CONFIG_PATH" ]; then
  CONFIG_PATH="$DEFAULT_CONFIG_PATH"
fi

CONFIG_DIR="$(dirname "$CONFIG_PATH")"

if [ -z "$WORKDIR_PATH" ]; then
  WORKDIR_PATH="$DEFAULT_WORKDIR"
fi

pick_uid_gid() {
  # Explicit override.
  if [ -n "${PRISM_UID:-}" ] || [ -n "${PRISM_GID:-}" ]; then
    uid="${PRISM_UID:-10001}"
    gid="${PRISM_GID:-10001}"
    echo "$uid:$gid"
    return
  fi

  # Prefer owner of config/workdir when they exist (bind mounts on Linux).
  for p in "$CONFIG_DIR" "$WORKDIR_PATH"; do
    if [ -e "$p" ]; then
      # busybox stat supports -c on Alpine.
      if uidgid="$(stat -c '%u:%g' "$p" 2>/dev/null)"; then
        echo "$uidgid"
        return
      fi
    fi
  done

  echo "10001:10001"
}

ensure_dir() {
  d="$1"
  mkdir -p "$d" 2>/dev/null || true
}

if [ "$#" -eq 0 ]; then
  set -- --config "$CONFIG_PATH"
fi

if [ "$(id -u)" -eq 0 ]; then
  uidgid="$(pick_uid_gid)"

  ensure_dir "$CONFIG_DIR"
  ensure_dir "$WORKDIR_PATH"

  # Best-effort ownership fixups; may fail on some bind mounts (e.g. Windows/OSX).
  chown -R "$uidgid" "$CONFIG_DIR" 2>/dev/null || true
  chown -R "$uidgid" "$WORKDIR_PATH" 2>/dev/null || true

  # Prefer dropping privileges if su-exec exists and the target UID can write where needed.
  if command -v su-exec >/dev/null 2>&1; then
    if su-exec "$uidgid" test -w "$CONFIG_DIR" && su-exec "$uidgid" test -w "$WORKDIR_PATH"; then
      exec su-exec "$uidgid" "$PRISM_BIN" "$@"
    fi
  fi

  # Fallback: check if running as root is explicitly permitted.
  if [ "${PRISM_ALLOW_ROOT:-0}" = "1" ]; then
    echo "WARNING: Prism container is running as root because dropping privileges to $uidgid failed." >&2
    echo "         Running as root is not recommended for production environments." >&2
    exec "$PRISM_BIN" "$@"
  else
    echo "ERROR: Prism container cannot drop privileges to non-root user ($uidgid) because $CONFIG_DIR or $WORKDIR_PATH is not writable by $uidgid." >&2
    echo "       Please fix permissions on your host bind mount, or set PRISM_ALLOW_ROOT=1 to explicitly allow running as root." >&2
    exit 1
  fi
fi

# Not root: just run Prism as-is.
exec "$PRISM_BIN" "$@"
