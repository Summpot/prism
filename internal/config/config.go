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

type Config struct {
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
	ListenAddr string `json:"listen_addr" yaml:"listen_addr" toml:"listen_addr"`
	AdminAddr  string `json:"admin_addr" yaml:"admin_addr" toml:"admin_addr"`
	Logging    *struct {
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
		ListenAddr: fc.ListenAddr,
		AdminAddr:  fc.AdminAddr,
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
