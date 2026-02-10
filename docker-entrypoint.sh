#!/bin/sh
set -eu

PRISM_BIN="/usr/local/bin/prism"

# Defaults (match Prism's runtime defaults on Linux).
DEFAULT_CONFIG_PATH="/etc/prism/prism.toml"
DEFAULT_WORKDIR="/var/lib/prism"

CONFIG_PATH="${PRISM_CONFIG:-}"
WORKDIR_PATH="${PRISM_WORKDIR:-}"
PARSER_DIR_PATH="${PRISM_ROUTING_PARSER_DIR:-}"

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
    --routing-parser-dir)
      PARSER_DIR_PATH="$arg"
      prev=""
      continue
      ;;
  esac

  case "$arg" in
    --config|--workdir|--routing-parser-dir)
      prev="$arg"
      ;;
    --config=*)
      CONFIG_PATH="${arg#*=}"
      ;;
    --workdir=*)
      WORKDIR_PATH="${arg#*=}"
      ;;
    --routing-parser-dir=*)
      PARSER_DIR_PATH="${arg#*=}"
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

if [ -z "$PARSER_DIR_PATH" ]; then
  PARSER_DIR_PATH="$CONFIG_DIR/parsers"
fi

# Keep semantics aligned with Prism:
# - PRISM_WORKDIR / --workdir relative paths are resolved against CWD.
# - PRISM_ROUTING_PARSER_DIR / --routing-parser-dir relative paths are resolved against config dir.
case "$PARSER_DIR_PATH" in
  /*) ;;
  *) PARSER_DIR_PATH="$CONFIG_DIR/$PARSER_DIR_PATH" ;;
esac

pick_uid_gid() {
  # Explicit override.
  if [ -n "${PRISM_UID:-}" ] || [ -n "${PRISM_GID:-}" ]; then
    uid="${PRISM_UID:-10001}"
    gid="${PRISM_GID:-10001}"
    echo "$uid:$gid"
    return
  fi

  # Prefer owner of config/workdir when they exist (bind mounts on Linux).
  for p in "$CONFIG_DIR" "$WORKDIR_PATH" "$PARSER_DIR_PATH"; do
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

if [ "$(id -u)" -eq 0 ]; then
  uidgid="$(pick_uid_gid)"

  ensure_dir "$CONFIG_DIR"
  ensure_dir "$WORKDIR_PATH"
  ensure_dir "$PARSER_DIR_PATH"

  # Best-effort ownership fixups; may fail on some bind mounts (e.g. Windows/OSX).
  chown -R "$uidgid" "$CONFIG_DIR" 2>/dev/null || true
  chown -R "$uidgid" "$WORKDIR_PATH" 2>/dev/null || true
  chown -R "$uidgid" "$PARSER_DIR_PATH" 2>/dev/null || true

  # Prefer dropping privileges if su-exec exists and the target UID can write where needed.
  if command -v su-exec >/dev/null 2>&1; then
    if su-exec "$uidgid" sh -c "test -w '$CONFIG_DIR' && test -w '$WORKDIR_PATH' && test -w '$PARSER_DIR_PATH'" 2>/dev/null; then
      exec su-exec "$uidgid" "$PRISM_BIN" "$@"
    fi
  fi

  # Fallback: run as root to avoid permission errors on bind mounts.
  exec "$PRISM_BIN" "$@"
fi

# Not root: just run Prism as-is.
exec "$PRISM_BIN" "$@"
