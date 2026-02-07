package config

import (
	"context"
	"encoding/json"
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

type TunnelServerConfig struct {
	Enabled    bool
	ListenAddr string
	// Transport is one of: tcp, udp, quic.
	Transport string
	// AuthToken is an optional shared secret required for prismc registration.
	AuthToken string

	QUIC struct {
		CertFile string
		KeyFile  string
	}
}

type TunnelClientServiceConfig struct {
	Name      string
	LocalAddr string
}

type TunnelClientQUICConfig struct {
	ServerName         string
	InsecureSkipVerify bool
}

type TunnelClientConfig struct {
	Enabled    bool
	ServerAddr string
	Transport  string
	AuthToken  string
	Services   []TunnelClientServiceConfig

	DialTimeout time.Duration
	QUIC        TunnelClientQUICConfig
}

type Config struct {
	// ServerEnabled controls whether Prism runs the data plane (TCP listener),
	// admin server, routing, and (optional) tunnel server.
	//
	// When false, Prism can still run as a tunnel client only.
	ServerEnabled bool

	ListenAddr string
	AdminAddr  string

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

	// Tunnel enables reverse-connection mode (prismc -> prisms) for reaching
	// private backends without public IPs. Routes can target services via the
	// upstream syntax tunnel:<service>.
	Tunnel TunnelServerConfig

	// TunnelClient runs a reverse tunnel client loop (like frp's client) that
	// dials a remote tunnel server and registers services.
	TunnelClient TunnelClientConfig
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
	ServerEnabled *bool  `json:"server_enabled" yaml:"server_enabled" toml:"server_enabled"`
	ListenAddr    string `json:"listen_addr" yaml:"listen_addr" toml:"listen_addr"`
	AdminAddr     string `json:"admin_addr" yaml:"admin_addr" toml:"admin_addr"`
	Logging       *struct {
		Level       string `json:"level" yaml:"level" toml:"level"`
		Format      string `json:"format" yaml:"format" toml:"format"`
		Output      string `json:"output" yaml:"output" toml:"output"`
		AddSource   bool   `json:"add_source" yaml:"add_source" toml:"add_source"`
		AdminBuffer *struct {
			Enabled bool `json:"enabled" yaml:"enabled" toml:"enabled"`
			Size    int  `json:"size" yaml:"size" toml:"size"`
		} `json:"admin_buffer" yaml:"admin_buffer" toml:"admin_buffer"`
	} `json:"logging" yaml:"logging" toml:"logging"`
	Routes map[string]string `json:"routes" yaml:"routes" toml:"routes"`

	RoutingParsers []struct {
		Type         string `json:"type" yaml:"type" toml:"type"`
		Name         string `json:"name" yaml:"name" toml:"name"`
		Path         string `json:"path" yaml:"path" toml:"path"`
		Function     string `json:"function" yaml:"function" toml:"function"`
		MaxOutputLen int    `json:"max_output_len" yaml:"max_output_len" toml:"max_output_len"`
	} `json:"routing_parsers" yaml:"routing_parsers" toml:"routing_parsers"`
	MaxHeaderBytes int `json:"max_header_bytes" yaml:"max_header_bytes" toml:"max_header_bytes"`

	Reload *struct {
		Enabled        bool `json:"enabled" yaml:"enabled" toml:"enabled"`
		PollIntervalMs int  `json:"poll_interval_ms" yaml:"poll_interval_ms" toml:"poll_interval_ms"`
	} `json:"reload" yaml:"reload" toml:"reload"`

	ProxyProtocolV2       bool `json:"proxy_protocol_v2" yaml:"proxy_protocol_v2" toml:"proxy_protocol_v2"`
	BufferSize            int  `json:"buffer_size" yaml:"buffer_size" toml:"buffer_size"`
	UpstreamDialTimeoutMs int  `json:"upstream_dial_timeout_ms" yaml:"upstream_dial_timeout_ms" toml:"upstream_dial_timeout_ms"`
	Timeouts              struct {
		HandshakeTimeoutMs int `json:"handshake_timeout_ms" yaml:"handshake_timeout_ms" toml:"handshake_timeout_ms"`
		IdleTimeoutMs      int `json:"idle_timeout_ms" yaml:"idle_timeout_ms" toml:"idle_timeout_ms"`
	} `json:"timeouts" yaml:"timeouts" toml:"timeouts"`

	Tunnel *struct {
		Enabled    bool   `json:"enabled" yaml:"enabled" toml:"enabled"`
		ListenAddr string `json:"listen_addr" yaml:"listen_addr" toml:"listen_addr"`
		Transport  string `json:"transport" yaml:"transport" toml:"transport"`
		AuthToken  string `json:"auth_token" yaml:"auth_token" toml:"auth_token"`
		QUIC       *struct {
			CertFile string `json:"cert_file" yaml:"cert_file" toml:"cert_file"`
			KeyFile  string `json:"key_file" yaml:"key_file" toml:"key_file"`
		} `json:"quic" yaml:"quic" toml:"quic"`
	} `json:"tunnel" yaml:"tunnel" toml:"tunnel"`

	TunnelClient *struct {
		Enabled       bool   `json:"enabled" yaml:"enabled" toml:"enabled"`
		ServerAddr    string `json:"server_addr" yaml:"server_addr" toml:"server_addr"`
		Transport     string `json:"transport" yaml:"transport" toml:"transport"`
		AuthToken     string `json:"auth_token" yaml:"auth_token" toml:"auth_token"`
		DialTimeoutMs int    `json:"dial_timeout_ms" yaml:"dial_timeout_ms" toml:"dial_timeout_ms"`
		QUIC          *struct {
			ServerName         string `json:"server_name" yaml:"server_name" toml:"server_name"`
			InsecureSkipVerify bool   `json:"insecure_skip_verify" yaml:"insecure_skip_verify" toml:"insecure_skip_verify"`
		} `json:"quic" yaml:"quic" toml:"quic"`
		Services []struct {
			Name      string `json:"name" yaml:"name" toml:"name"`
			LocalAddr string `json:"local_addr" yaml:"local_addr" toml:"local_addr"`
		} `json:"services" yaml:"services" toml:"services"`
	} `json:"tunnel_client" yaml:"tunnel_client" toml:"tunnel_client"`
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
		ServerEnabled: true,
		ListenAddr:    fc.ListenAddr,
		AdminAddr:     fc.AdminAddr,
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
		Tunnel: TunnelServerConfig{
			Enabled:   false,
			Transport: "tcp",
		},
		TunnelClient: TunnelClientConfig{
			Enabled:     false,
			Transport:   "tcp",
			DialTimeout: 5 * time.Second,
			Services:    nil,
		},
	}
	if fc.ServerEnabled != nil {
		cfg.ServerEnabled = *fc.ServerEnabled
	}
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

	if cfg.ListenAddr == "" {
		cfg.ListenAddr = ":25565"
	}
	if cfg.AdminAddr == "" {
		cfg.AdminAddr = ":8080"
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

	if fc.Tunnel != nil {
		cfg.Tunnel.Enabled = fc.Tunnel.Enabled
		if fc.Tunnel.ListenAddr != "" {
			cfg.Tunnel.ListenAddr = fc.Tunnel.ListenAddr
		}
		if fc.Tunnel.Transport != "" {
			cfg.Tunnel.Transport = fc.Tunnel.Transport
		}
		cfg.Tunnel.AuthToken = fc.Tunnel.AuthToken
		if fc.Tunnel.QUIC != nil {
			cfg.Tunnel.QUIC.CertFile = fc.Tunnel.QUIC.CertFile
			cfg.Tunnel.QUIC.KeyFile = fc.Tunnel.QUIC.KeyFile
		}
	}

	if fc.TunnelClient != nil {
		cfg.TunnelClient.Enabled = fc.TunnelClient.Enabled
		cfg.TunnelClient.ServerAddr = strings.TrimSpace(fc.TunnelClient.ServerAddr)
		if strings.TrimSpace(fc.TunnelClient.Transport) != "" {
			cfg.TunnelClient.Transport = strings.TrimSpace(fc.TunnelClient.Transport)
		}
		cfg.TunnelClient.AuthToken = fc.TunnelClient.AuthToken
		if fc.TunnelClient.DialTimeoutMs > 0 {
			cfg.TunnelClient.DialTimeout = time.Duration(fc.TunnelClient.DialTimeoutMs) * time.Millisecond
		}
		if fc.TunnelClient.QUIC != nil {
			cfg.TunnelClient.QUIC.ServerName = strings.TrimSpace(fc.TunnelClient.QUIC.ServerName)
			cfg.TunnelClient.QUIC.InsecureSkipVerify = fc.TunnelClient.QUIC.InsecureSkipVerify
		}
		if len(fc.TunnelClient.Services) > 0 {
			cfg.TunnelClient.Services = make([]TunnelClientServiceConfig, 0, len(fc.TunnelClient.Services))
			for _, s := range fc.TunnelClient.Services {
				name := strings.TrimSpace(s.Name)
				addr := strings.TrimSpace(s.LocalAddr)
				if name == "" || addr == "" {
					continue
				}
				cfg.TunnelClient.Services = append(cfg.TunnelClient.Services, TunnelClientServiceConfig{Name: name, LocalAddr: addr})
			}
		}
	}

	return cfg, nil
}

func unmarshalConfigFile(path string, data []byte, dst any) error {
	ext := strings.ToLower(filepath.Ext(path))
	switch ext {
	case ".json":
		return json.Unmarshal(data, dst)
	case ".yaml", ".yml":
		return yaml.Unmarshal(data, dst)
	case ".toml":
		// BurntSushi/toml works with string or io.Reader; this keeps things simple.
		_, err := toml.Decode(string(data), dst)
		return err
	default:
		return fmt.Errorf("unsupported config extension %q (expected .toml, .yaml/.yml, or .json)", ext)
	}
}
