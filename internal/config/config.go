package config

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/BurntSushi/toml"
	"gopkg.in/yaml.v3"
)

type Timeouts struct {
	HandshakeTimeout time.Duration
	IdleTimeout      time.Duration
}

// ProxyListenerConfig configures a single public-facing listener.
//
// TCP listeners can run in routing mode (hostname-based via routing_parsers + routes)
// or forward mode (fixed upstream).
//
// UDP listeners run in forward mode only.
type ProxyListenerConfig struct {
	ListenAddr string
	// Protocol is one of: tcp, udp.
	Protocol string
	// Mode is one of: routing, forward.
	// For udp it is implicitly forward.
	Mode string
	// Upstream is required for forward mode. It may be a dial address (host:port)
	// or a tunnel target (tunnel:<service>).
	Upstream string
}

type ReloadConfig struct {
	Enabled      bool
	PollInterval time.Duration
}

type AdminLogBufferConfig struct {
	Enabled bool
	Size    int
}

type LoggingConfig struct {
	// Level is one of: debug, info, warn, error.
	Level string
	// Format is one of: json, text.
	Format string
	// Output is one of: stderr, stdout, discard; or a file path.
	Output string
	// AddSource enables source file/line reporting (slightly higher overhead).
	AddSource bool
	// AdminBuffer controls an in-memory log line ring buffer used by the admin server.
	AdminBuffer AdminLogBufferConfig
}

type RoutingParserConfig struct {
	Type         string
	Name         string
	Path         string
	Function     string
	MaxOutputLen int
}

type TunnelClientServiceConfig struct {
	Name string
	// Proto is one of: tcp, udp. Defaults to tcp.
	Proto string
	// LocalAddr is the local backend address on the tunnel client.
	LocalAddr string
	// RemoteAddr (optional) requests the tunnel server to open a public listener
	// for this service (frp-like behavior). Example: ":25565".
	RemoteAddr string
}

type TunnelClientQUICConfig struct {
	ServerName         string
	InsecureSkipVerify bool
}

type TunnelListenerConfig struct {
	// ListenAddr is the address the tunnel server listens on.
	//
	// The presence of one or more listeners enables the tunnel server role.
	ListenAddr string
	// Transport is one of: tcp, udp, quic.
	Transport string
	QUIC      struct {
		CertFile string
		KeyFile  string
	}
}

type TunnelClientConnectConfig struct {
	ServerAddr  string
	Transport   string
	DialTimeout time.Duration
	QUIC        TunnelClientQUICConfig
}

type TunnelConfig struct {
	// AuthToken is an optional shared secret required for client registration.
	AuthToken string

	// AutoListenServices enables frp-like behavior on the tunnel server: when a
	// tunnel client registers a service with a RemoteAddr, prisms will
	// automatically open a server-side listener for that service.
	AutoListenServices bool

	// Listeners configures one or more tunnel server listeners. Multiple entries
	// allow serving multiple transports at the same time (similar to frps).
	Listeners []TunnelListenerConfig

	// Client configures the tunnel client role (optional). If Client is present
	// and Services is non-empty, Prism runs the tunnel client loop.
	Client   *TunnelClientConnectConfig
	Services []TunnelClientServiceConfig
}

type Config struct {
	// ListenAddr is a legacy single-listener configuration.
	//
	// Prefer Listeners for new configs. When listen_addr is set (or inferred),
	// it is mapped into Listeners as a TCP routing listener.
	ListenAddr string
	// Listeners configures one or more proxy listeners (multi-port / multi-protocol).
	Listeners []ProxyListenerConfig
	// AdminAddr enables the admin HTTP server when non-empty.
	AdminAddr string

	Logging LoggingConfig

	Routes map[string]string

	// RoutingParsers controls how Prism extracts the routing hostname from the
	// first bytes of the stream.
	RoutingParsers []RoutingParserConfig
	MaxHeaderBytes int

	Reload ReloadConfig

	ProxyProtocolV2     bool
	BufferSize          int
	UpstreamDialTimeout time.Duration
	Timeouts            Timeouts

	// Tunnel enables reverse-connection mode (client -> server) for reaching
	// private backends without public IPs. Routes can target services via the
	// upstream syntax tunnel:<service>.
	Tunnel TunnelConfig
}

type ConfigProvider interface {
	Load(ctx context.Context) (*Config, error)
}

type FileConfigProvider struct {
	Path string
}

func NewFileConfigProvider(path string) *FileConfigProvider {
	return &FileConfigProvider{Path: path}
}

func (p *FileConfigProvider) WatchPath() string {
	return p.Path
}

type fileConfig struct {
	// ServerEnabled is deprecated. It is kept for backward compatibility but
	// should not be used in new configs.
	ServerEnabled *bool `yaml:"server_enabled" toml:"server_enabled"`

	ListenAddr *string `yaml:"listen_addr" toml:"listen_addr"`
	Listeners  []struct {
		ListenAddr string `yaml:"listen_addr" toml:"listen_addr"`
		Protocol   string `yaml:"protocol" toml:"protocol"`
		Mode       string `yaml:"mode" toml:"mode"`
		Upstream   string `yaml:"upstream" toml:"upstream"`
	} `yaml:"listeners" toml:"listeners"`
	AdminAddr *string `yaml:"admin_addr" toml:"admin_addr"`
	Logging   *struct {
		Level       string `yaml:"level" toml:"level"`
		Format      string `yaml:"format" toml:"format"`
		Output      string `yaml:"output" toml:"output"`
		AddSource   bool   `yaml:"add_source" toml:"add_source"`
		AdminBuffer *struct {
			Enabled bool `yaml:"enabled" toml:"enabled"`
			Size    int  `yaml:"size" toml:"size"`
		} `yaml:"admin_buffer" toml:"admin_buffer"`
	} `yaml:"logging" toml:"logging"`
	Routes map[string]string `yaml:"routes" toml:"routes"`

	RoutingParsers []struct {
		Type         string `yaml:"type" toml:"type"`
		Name         string `yaml:"name" toml:"name"`
		Path         string `yaml:"path" toml:"path"`
		Function     string `yaml:"function" toml:"function"`
		MaxOutputLen int    `yaml:"max_output_len" toml:"max_output_len"`
	} `yaml:"routing_parsers" toml:"routing_parsers"`
	MaxHeaderBytes int `yaml:"max_header_bytes" toml:"max_header_bytes"`

	Reload *struct {
		Enabled        bool `yaml:"enabled" toml:"enabled"`
		PollIntervalMs int  `yaml:"poll_interval_ms" toml:"poll_interval_ms"`
	} `yaml:"reload" toml:"reload"`

	ProxyProtocolV2       bool `yaml:"proxy_protocol_v2" toml:"proxy_protocol_v2"`
	BufferSize            int  `yaml:"buffer_size" toml:"buffer_size"`
	UpstreamDialTimeoutMs int  `yaml:"upstream_dial_timeout_ms" toml:"upstream_dial_timeout_ms"`
	Timeouts              struct {
		HandshakeTimeoutMs int `yaml:"handshake_timeout_ms" toml:"handshake_timeout_ms"`
		IdleTimeoutMs      int `yaml:"idle_timeout_ms" toml:"idle_timeout_ms"`
	} `yaml:"timeouts" toml:"timeouts"`

	Tunnel *struct {
		// New schema.
		AuthToken          string `yaml:"auth_token" toml:"auth_token"`
		AutoListenServices *bool  `yaml:"auto_listen_services" toml:"auto_listen_services"`
		Listeners          []struct {
			Transport  string `yaml:"transport" toml:"transport"`
			ListenAddr string `yaml:"listen_addr" toml:"listen_addr"`
			QUIC       *struct {
				CertFile string `yaml:"cert_file" toml:"cert_file"`
				KeyFile  string `yaml:"key_file" toml:"key_file"`
			} `yaml:"quic" toml:"quic"`
		} `yaml:"listeners" toml:"listeners"`
		Client *struct {
			ServerAddr    string `yaml:"server_addr" toml:"server_addr"`
			Transport     string `yaml:"transport" toml:"transport"`
			DialTimeoutMs int    `yaml:"dial_timeout_ms" toml:"dial_timeout_ms"`
			QUIC          *struct {
				ServerName         string `yaml:"server_name" toml:"server_name"`
				InsecureSkipVerify bool   `yaml:"insecure_skip_verify" toml:"insecure_skip_verify"`
			} `yaml:"quic" toml:"quic"`
		} `yaml:"client" toml:"client"`
		Services []struct {
			Name       string `yaml:"name" toml:"name"`
			Proto      string `yaml:"proto" toml:"proto"`
			LocalAddr  string `yaml:"local_addr" toml:"local_addr"`
			RemoteAddr string `yaml:"remote_addr" toml:"remote_addr"`
		} `yaml:"services" toml:"services"`

		// Legacy schema (deprecated): a single tunnel server listener.
		Enabled    *bool  `yaml:"enabled" toml:"enabled"`
		ListenAddr string `yaml:"listen_addr" toml:"listen_addr"`
		Transport  string `yaml:"transport" toml:"transport"`
		QUIC       *struct {
			CertFile string `yaml:"cert_file" toml:"cert_file"`
			KeyFile  string `yaml:"key_file" toml:"key_file"`
		} `yaml:"quic" toml:"quic"`
	} `yaml:"tunnel" toml:"tunnel"`

	// Legacy schema (deprecated): tunnel client block.
	TunnelClient *struct {
		Enabled       *bool  `yaml:"enabled" toml:"enabled"`
		ServerAddr    string `yaml:"server_addr" toml:"server_addr"`
		Transport     string `yaml:"transport" toml:"transport"`
		AuthToken     string `yaml:"auth_token" toml:"auth_token"`
		DialTimeoutMs int    `yaml:"dial_timeout_ms" toml:"dial_timeout_ms"`
		QUIC          *struct {
			ServerName         string `yaml:"server_name" toml:"server_name"`
			InsecureSkipVerify bool   `yaml:"insecure_skip_verify" toml:"insecure_skip_verify"`
		} `yaml:"quic" toml:"quic"`
		Services []struct {
			Name       string `yaml:"name" toml:"name"`
			Proto      string `yaml:"proto" toml:"proto"`
			LocalAddr  string `yaml:"local_addr" toml:"local_addr"`
			RemoteAddr string `yaml:"remote_addr" toml:"remote_addr"`
		} `yaml:"services" toml:"services"`
	} `yaml:"tunnel_client" toml:"tunnel_client"`
}

func (p *FileConfigProvider) Load(_ context.Context) (*Config, error) {
	data, err := os.ReadFile(p.Path)
	if err != nil {
		return nil, err
	}

	var fc fileConfig
	if err := unmarshalConfigFile(p.Path, data, &fc); err != nil {
		return nil, fmt.Errorf("parse %s: %w", p.Path, err)
	}

	cfg := &Config{
		ListenAddr: "",
		Listeners:  nil,
		AdminAddr:  "",
		Logging: LoggingConfig{
			Level:  "info",
			Format: "json",
			Output: "stderr",
			AdminBuffer: AdminLogBufferConfig{
				Enabled: false,
				Size:    1000,
			},
		},
		Routes:              fc.Routes,
		MaxHeaderBytes:      fc.MaxHeaderBytes,
		ProxyProtocolV2:     fc.ProxyProtocolV2,
		BufferSize:          fc.BufferSize,
		UpstreamDialTimeout: time.Duration(fc.UpstreamDialTimeoutMs) * time.Millisecond,
		Timeouts: Timeouts{
			HandshakeTimeout: time.Duration(fc.Timeouts.HandshakeTimeoutMs) * time.Millisecond,
			IdleTimeout:      time.Duration(fc.Timeouts.IdleTimeoutMs) * time.Millisecond,
		},
		Reload: ReloadConfig{},
		Tunnel: TunnelConfig{AutoListenServices: true},
	}

	// --- Logging / reload / parsers ---
	if fc.Logging != nil {
		if fc.Logging.Level != "" {
			cfg.Logging.Level = fc.Logging.Level
		}
		if fc.Logging.Format != "" {
			cfg.Logging.Format = fc.Logging.Format
		}
		if fc.Logging.Output != "" {
			cfg.Logging.Output = fc.Logging.Output
		}
		cfg.Logging.AddSource = fc.Logging.AddSource
		if fc.Logging.AdminBuffer != nil {
			cfg.Logging.AdminBuffer.Enabled = fc.Logging.AdminBuffer.Enabled
			if fc.Logging.AdminBuffer.Size != 0 {
				cfg.Logging.AdminBuffer.Size = fc.Logging.AdminBuffer.Size
			}
		}
	}
	if fc.Reload == nil {
		cfg.Reload.Enabled = true
	} else {
		cfg.Reload.Enabled = fc.Reload.Enabled
		cfg.Reload.PollInterval = time.Duration(fc.Reload.PollIntervalMs) * time.Millisecond
	}

	if len(fc.RoutingParsers) > 0 {
		cfg.RoutingParsers = make([]RoutingParserConfig, 0, len(fc.RoutingParsers))
		for _, rp := range fc.RoutingParsers {
			cfg.RoutingParsers = append(cfg.RoutingParsers, RoutingParserConfig{
				Type:         rp.Type,
				Name:         rp.Name,
				Path:         rp.Path,
				Function:     rp.Function,
				MaxOutputLen: rp.MaxOutputLen,
			})
		}
	}

	// --- Tunnel config (new schema) + legacy mapping ---
	var tun TunnelConfig
	if fc.Tunnel != nil {
		tun.AuthToken = strings.TrimSpace(fc.Tunnel.AuthToken)
		tun.AutoListenServices = true
		if fc.Tunnel.AutoListenServices != nil {
			tun.AutoListenServices = *fc.Tunnel.AutoListenServices
		}

		// New: multiple listeners.
		if len(fc.Tunnel.Listeners) > 0 {
			tun.Listeners = make([]TunnelListenerConfig, 0, len(fc.Tunnel.Listeners))
			for _, l := range fc.Tunnel.Listeners {
				la := strings.TrimSpace(l.ListenAddr)
				if la == "" {
					return nil, fmt.Errorf("config: tunnel.listeners entry missing listen_addr")
				}
				tr := strings.TrimSpace(l.Transport)
				if tr == "" {
					tr = "tcp"
				}
				lc := TunnelListenerConfig{ListenAddr: la, Transport: tr}
				if l.QUIC != nil {
					lc.QUIC.CertFile = strings.TrimSpace(l.QUIC.CertFile)
					lc.QUIC.KeyFile = strings.TrimSpace(l.QUIC.KeyFile)
				}
				tun.Listeners = append(tun.Listeners, lc)
			}
		}

		// New: client.
		if fc.Tunnel.Client != nil {
			cc := &TunnelClientConnectConfig{}
			cc.ServerAddr = strings.TrimSpace(fc.Tunnel.Client.ServerAddr)
			cc.Transport = strings.TrimSpace(fc.Tunnel.Client.Transport)
			if cc.Transport == "" {
				cc.Transport = "tcp"
			}
			if fc.Tunnel.Client.DialTimeoutMs > 0 {
				cc.DialTimeout = time.Duration(fc.Tunnel.Client.DialTimeoutMs) * time.Millisecond
			} else {
				cc.DialTimeout = 5 * time.Second
			}
			if fc.Tunnel.Client.QUIC != nil {
				cc.QUIC.ServerName = strings.TrimSpace(fc.Tunnel.Client.QUIC.ServerName)
				cc.QUIC.InsecureSkipVerify = fc.Tunnel.Client.QUIC.InsecureSkipVerify
			}
			tun.Client = cc
		}

		if len(fc.Tunnel.Services) > 0 {
			tun.Services = make([]TunnelClientServiceConfig, 0, len(fc.Tunnel.Services))
			for _, s := range fc.Tunnel.Services {
				name := strings.TrimSpace(s.Name)
				proto := strings.TrimSpace(strings.ToLower(s.Proto))
				addr := strings.TrimSpace(s.LocalAddr)
				remote := strings.TrimSpace(s.RemoteAddr)
				if name == "" || addr == "" {
					continue
				}
				if proto == "" {
					proto = "tcp"
				}
				switch proto {
				case "tcp", "udp":
				default:
					return nil, fmt.Errorf("config: tunnel.services entry %q has invalid proto %q", name, proto)
				}
				tun.Services = append(tun.Services, TunnelClientServiceConfig{Name: name, Proto: proto, LocalAddr: addr, RemoteAddr: remote})
			}
		}

		// Legacy: single tunnel server config (enabled/listen_addr/transport).
		legacyOn := true
		if fc.Tunnel.Enabled != nil {
			legacyOn = *fc.Tunnel.Enabled
		}
		if legacyOn {
			if la := strings.TrimSpace(fc.Tunnel.ListenAddr); la != "" {
				tr := strings.TrimSpace(fc.Tunnel.Transport)
				if tr == "" {
					tr = "tcp"
				}
				lc := TunnelListenerConfig{ListenAddr: la, Transport: tr}
				if fc.Tunnel.QUIC != nil {
					lc.QUIC.CertFile = strings.TrimSpace(fc.Tunnel.QUIC.CertFile)
					lc.QUIC.KeyFile = strings.TrimSpace(fc.Tunnel.QUIC.KeyFile)
				}
				tun.Listeners = append(tun.Listeners, lc)
			}
		}
	}

	// Legacy: tunnel_client block.
	if fc.TunnelClient != nil {
		legacyOn := true
		if fc.TunnelClient.Enabled != nil {
			legacyOn = *fc.TunnelClient.Enabled
		}
		if legacyOn {
			// Auth token.
			if tun.AuthToken == "" {
				tun.AuthToken = strings.TrimSpace(fc.TunnelClient.AuthToken)
			}
			// Client.
			if tun.Client == nil {
				cc := &TunnelClientConnectConfig{}
				cc.ServerAddr = strings.TrimSpace(fc.TunnelClient.ServerAddr)
				cc.Transport = strings.TrimSpace(fc.TunnelClient.Transport)
				if cc.Transport == "" {
					cc.Transport = "tcp"
				}
				if fc.TunnelClient.DialTimeoutMs > 0 {
					cc.DialTimeout = time.Duration(fc.TunnelClient.DialTimeoutMs) * time.Millisecond
				} else {
					cc.DialTimeout = 5 * time.Second
				}
				if fc.TunnelClient.QUIC != nil {
					cc.QUIC.ServerName = strings.TrimSpace(fc.TunnelClient.QUIC.ServerName)
					cc.QUIC.InsecureSkipVerify = fc.TunnelClient.QUIC.InsecureSkipVerify
				}
				tun.Client = cc
			}

			// Services.
			if len(fc.TunnelClient.Services) > 0 {
				if tun.Services == nil {
					tun.Services = make([]TunnelClientServiceConfig, 0, len(fc.TunnelClient.Services))
				}
				for _, s := range fc.TunnelClient.Services {
					name := strings.TrimSpace(s.Name)
					proto := strings.TrimSpace(strings.ToLower(s.Proto))
					addr := strings.TrimSpace(s.LocalAddr)
					remote := strings.TrimSpace(s.RemoteAddr)
					if name == "" || addr == "" {
						continue
					}
					if proto == "" {
						proto = "tcp"
					}
					switch proto {
					case "tcp", "udp":
					default:
						return nil, fmt.Errorf("config: tunnel_client.services entry %q has invalid proto %q", name, proto)
					}
					tun.Services = append(tun.Services, TunnelClientServiceConfig{Name: name, Proto: proto, LocalAddr: addr, RemoteAddr: remote})
				}
			}
		}
	}

	cfg.Tunnel = tun

	// --- Proxy listeners (multi-port / multi-protocol) ---
	var listeners []ProxyListenerConfig
	if len(fc.Listeners) > 0 {
		listeners = make([]ProxyListenerConfig, 0, len(fc.Listeners))
		for i, l := range fc.Listeners {
			la := strings.TrimSpace(l.ListenAddr)
			if la == "" {
				return nil, fmt.Errorf("config: listeners[%d] missing listen_addr", i)
			}
			proto := strings.TrimSpace(strings.ToLower(l.Protocol))
			if proto == "" {
				proto = "tcp"
			}
			mode := strings.TrimSpace(strings.ToLower(l.Mode))
			up := strings.TrimSpace(l.Upstream)

			switch proto {
			case "tcp":
				if mode == "" {
					if up == "" {
						mode = "routing"
					} else {
						mode = "forward"
					}
				}
				switch mode {
				case "routing":
					if up != "" {
						return nil, fmt.Errorf("config: listeners[%d] mode=routing cannot set upstream", i)
					}
				case "forward":
					if up == "" {
						return nil, fmt.Errorf("config: listeners[%d] mode=forward requires upstream", i)
					}
				default:
					return nil, fmt.Errorf("config: listeners[%d] has invalid mode %q", i, mode)
				}
			case "udp":
				// UDP only supports forward mode.
				if mode != "" && mode != "forward" {
					return nil, fmt.Errorf("config: listeners[%d] protocol=udp only supports mode=forward", i)
				}
				mode = "forward"
				if up == "" {
					return nil, fmt.Errorf("config: listeners[%d] protocol=udp requires upstream", i)
				}
			default:
				return nil, fmt.Errorf("config: listeners[%d] has invalid protocol %q", i, proto)
			}

			listeners = append(listeners, ProxyListenerConfig{ListenAddr: la, Protocol: proto, Mode: mode, Upstream: up})
		}
	}

	// --- Defaults and inferred enablement ---
	var listenAddr string
	if fc.ListenAddr != nil {
		listenAddr = strings.TrimSpace(*fc.ListenAddr)
	}
	var adminAddr string
	if fc.AdminAddr != nil {
		adminAddr = strings.TrimSpace(*fc.AdminAddr)
	}

	// Infer whether the proxy server should run.
	proxyEnabled := false
	if fc.ServerEnabled != nil {
		// Deprecated explicit override.
		proxyEnabled = *fc.ServerEnabled
	} else {
		proxyEnabled = len(listeners) > 0 || listenAddr != "" || len(cfg.Routes) > 0
	}

	// Prevent confusing states: routes imply a listener unless explicit multi-listener config is used.
	if len(cfg.Routes) > 0 && fc.ListenAddr != nil && listenAddr == "" && len(listeners) == 0 {
		return nil, fmt.Errorf("config: routes are set but listen_addr is empty")
	}

	if proxyEnabled {
		// Legacy listen_addr maps to a TCP routing listener.
		if listenAddr != "" {
			dup := false
			for _, l := range listeners {
				if strings.TrimSpace(l.ListenAddr) == listenAddr && strings.TrimSpace(strings.ToLower(l.Protocol)) == "tcp" && strings.TrimSpace(strings.ToLower(l.Mode)) == "routing" {
					dup = true
					break
				}
			}
			if !dup {
				listeners = append(listeners, ProxyListenerConfig{ListenAddr: listenAddr, Protocol: "tcp", Mode: "routing"})
			}
		}
		if len(listeners) == 0 {
			// Backward compatible default: routes imply a default TCP listener.
			listeners = append(listeners, ProxyListenerConfig{ListenAddr: ":25565", Protocol: "tcp", Mode: "routing"})
		}
		cfg.Listeners = listeners
		// Preserve legacy field for logs and default port inference.
		cfg.ListenAddr = cfg.Listeners[0].ListenAddr
		if fc.AdminAddr == nil {
			cfg.AdminAddr = ":8080"
		} else {
			// Explicit empty disables admin.
			cfg.AdminAddr = adminAddr
		}
	} else {
		// Proxy role disabled.
		if fc.AdminAddr != nil {
			cfg.AdminAddr = adminAddr
		}

		// If the tunnel server is configured and admin_addr was not specified,
		// keep the old behavior of defaulting the admin server on server roles.
		if cfg.AdminAddr == "" && fc.AdminAddr == nil && len(cfg.Tunnel.Listeners) > 0 {
			cfg.AdminAddr = ":8080"
		}
	}
	// Logging defaults are set above.
	if cfg.BufferSize <= 0 {
		cfg.BufferSize = 32 * 1024
	}
	if cfg.UpstreamDialTimeout <= 0 {
		cfg.UpstreamDialTimeout = 5 * time.Second
	}
	if cfg.Timeouts.HandshakeTimeout <= 0 {
		cfg.Timeouts.HandshakeTimeout = 3 * time.Second
	}
	if cfg.Routes == nil {
		cfg.Routes = map[string]string{}
	}
	if cfg.MaxHeaderBytes <= 0 {
		cfg.MaxHeaderBytes = 64 * 1024
	}
	if cfg.Reload.PollInterval <= 0 {
		cfg.Reload.PollInterval = 1 * time.Second
	}
	if len(cfg.RoutingParsers) == 0 {
		// Default: support Minecraft hostname routing and TLS SNI.
		cfg.RoutingParsers = []RoutingParserConfig{
			{Type: "builtin", Name: "minecraft_handshake"},
			{Type: "builtin", Name: "tls_sni"},
		}
	}

	return cfg, nil
}

func unmarshalConfigFile(path string, data []byte, dst any) error {
	ext := strings.ToLower(filepath.Ext(path))
	switch ext {
	case ".yaml", ".yml":
		return yaml.Unmarshal(data, dst)
	case ".toml":
		// BurntSushi/toml works with string or io.Reader; this keeps things simple.
		_, err := toml.Decode(string(data), dst)
		return err
	default:
		return fmt.Errorf("unsupported config extension %q (expected .toml or .yaml/.yml)", ext)
	}
}
