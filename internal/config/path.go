package config

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
)

// EnvConfigPath is the environment variable used to override the config file path.
const EnvConfigPath = "PRISM_CONFIG"

type ConfigPathSource string

const (
	ConfigPathSourceFlag    ConfigPathSource = "flag"
	ConfigPathSourceEnv     ConfigPathSource = "env"
	ConfigPathSourceCWD     ConfigPathSource = "cwd"
	ConfigPathSourceDefault ConfigPathSource = "default"
)

type ResolvedConfigPath struct {
	Path   string
	Source ConfigPathSource
}

// ResolveConfigPath resolves the effective configuration file path.
//
// Precedence:
//  1. explicitFlagPath (from -config)
//  2. PRISM_CONFIG environment variable
//  3. Auto-discovery in the current working directory (prism.toml > prism.yaml > prism.yml)
//  4. OS-specific default user config location
func ResolveConfigPath(explicitFlagPath string) (ResolvedConfigPath, error) {
	if p := strings.TrimSpace(explicitFlagPath); p != "" {
		p, err := normalizeExplicitPath(p)
		if err != nil {
			return ResolvedConfigPath{}, err
		}
		return ResolvedConfigPath{Path: p, Source: ConfigPathSourceFlag}, nil
	}

	if p := strings.TrimSpace(os.Getenv(EnvConfigPath)); p != "" {
		p, err := normalizeExplicitPath(p)
		if err != nil {
			return ResolvedConfigPath{}, err
		}
		return ResolvedConfigPath{Path: p, Source: ConfigPathSourceEnv}, nil
	}

	if p, err := DiscoverConfigPath("."); err == nil {
		return ResolvedConfigPath{Path: p, Source: ConfigPathSourceCWD}, nil
	}

	p, err := DefaultConfigPath()
	if err != nil {
		return ResolvedConfigPath{}, err
	}
	return ResolvedConfigPath{Path: p, Source: ConfigPathSourceDefault}, nil
}

func normalizeExplicitPath(p string) (string, error) {
	p = filepath.Clean(strings.TrimSpace(p))
	if p == "" {
		return "", fmt.Errorf("config: empty config path")
	}

	fi, err := os.Stat(p)
	if err == nil {
		if fi.IsDir() {
			// If a directory is provided, try to discover prism.* inside it; otherwise
			// default to prism.toml within that directory.
			if discovered, derr := DiscoverConfigPath(p); derr == nil {
				return discovered, nil
			}
			return filepath.Join(p, "prism.toml"), nil
		}
		// Existing file path: keep as-is.
		return p, nil
	}
	if err != nil && !os.IsNotExist(err) {
		return "", fmt.Errorf("config: stat %s: %w", p, err)
	}

	// For a new (non-existing) file path without an extension, default to TOML.
	if filepath.Ext(p) == "" {
		p += ".toml"
	}
	return p, nil
}

// DefaultConfigPath returns Prism's OS-specific default config file path.
//
// It uses os.UserConfigDir() (e.g. %AppData% on Windows, ~/.config on Linux,
// ~/Library/Application Support on macOS) and then appends prism/prism.toml.
func DefaultConfigPath() (string, error) {
	dir, err := os.UserConfigDir()
	if err != nil {
		return "", fmt.Errorf("config: resolve user config dir: %w", err)
	}
	dir = strings.TrimSpace(dir)
	if dir == "" {
		return "", fmt.Errorf("config: resolve user config dir: empty")
	}
	return filepath.Join(dir, "prism", "prism.toml"), nil
}

// EnsureConfigFile creates a new config file at path if it does not already exist.
// It never overwrites an existing regular file.
func EnsureConfigFile(path string) (created bool, err error) {
	path = strings.TrimSpace(path)
	if path == "" {
		return false, fmt.Errorf("config: empty config path")
	}

	fi, statErr := os.Stat(path)
	if statErr == nil {
		if fi.Mode().IsRegular() {
			return false, nil
		}
		return false, fmt.Errorf("config: %s exists but is not a regular file", path)
	}
	if statErr != nil && !os.IsNotExist(statErr) {
		return false, fmt.Errorf("config: stat %s: %w", path, statErr)
	}

	tmpl, err := defaultConfigTemplateForPath(path)
	if err != nil {
		return false, err
	}

	if dir := filepath.Dir(path); dir != "" && dir != "." {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return false, fmt.Errorf("config: mkdir %s: %w", dir, err)
		}
	}

	// Use O_EXCL to avoid clobbering files created concurrently.
	f, err := os.OpenFile(path, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o600)
	if err != nil {
		if os.IsExist(err) {
			return false, nil
		}
		return false, fmt.Errorf("config: create %s: %w", path, err)
	}
	defer func() { _ = f.Close() }()

	if _, err := io.WriteString(f, tmpl); err != nil {
		return false, fmt.Errorf("config: write %s: %w", path, err)
	}
	return true, nil
}

func defaultConfigTemplateForPath(path string) (string, error) {
	ext := strings.ToLower(filepath.Ext(path))
	switch ext {
	case ".toml":
		return defaultConfigTemplateTOML, nil
	case ".yaml", ".yml":
		return defaultConfigTemplateYAML, nil
	default:
		return "", fmt.Errorf("config: unsupported config extension %q (expected .toml or .yaml/.yml)", ext)
	}
}

const defaultConfigTemplateTOML = `# Prism configuration (auto-generated)
#
# This file was created because Prism could not find a configuration file at the
# resolved config path.
#
# This default config is meant to be runnable without edits and is focused on
# tunnel mode (frp-like): Prism starts a tunnel server and waits for clients to
# connect and register services.
#
# To expose a service to the public internet, configure the tunnel client with a
# service remote_addr (for example ":25565"); Prism will auto-listen on that port
# on the server side.

admin_addr = ":8080"

[tunnel]
auth_token = ""
auto_listen_services = true

[[tunnel.endpoints]]
listen_addr = ":7000"
transport = "tcp" # tcp | udp | quic

[logging]
level = "info"
format = "json"
output = "stderr"
add_source = false

[logging.admin_buffer]
enabled = true
size = 1000

[reload]
enabled = true
poll_interval_ms = 1000

[timeouts]
handshake_timeout_ms = 3000
idle_timeout_ms = 0

`

const defaultConfigTemplateYAML = `# Prism configuration (auto-generated)
#
# This file was created because Prism could not find a configuration file at the
# resolved config path.
#
# This default config is meant to be runnable without edits and is focused on
# tunnel mode (frp-like): Prism starts a tunnel server and waits for clients to
# connect and register services.
#
# To expose a service to the public internet, configure the tunnel client with a
# service remote_addr (for example ":25565"); Prism will auto-listen on that port
# on the server side.

admin_addr: ":8080"

tunnel:

  auth_token: ""
  auto_listen_services: true
  endpoints:
    - listen_addr: ":7000"
      transport: "tcp" # tcp | udp | quic

logging:

  level: "info"
  format: "json"
  output: "stderr"
  add_source: false
  admin_buffer:
    enabled: true
    size: 1000

reload:

  enabled: true
  poll_interval_ms: 1000

timeouts:

  handshake_timeout_ms: 3000
  idle_timeout_ms: 0

`
