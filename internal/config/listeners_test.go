package config

import (
	"context"
	"os"
	"path/filepath"
	"testing"
)

func TestFileConfigProvider_Load_ListenersYAML(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "prism.yaml")

	if err := os.WriteFile(path, []byte(`
listeners:
  - listen_addr: ":25565"
    protocol: "tcp"
  - listen_addr: ":19132"
    protocol: "udp"
    upstream: "127.0.0.1:19132"

routes:
  play.example.com: "127.0.0.1:25566"
`), 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	p := NewFileConfigProvider(path)
	cfg, err := p.Load(context.Background())
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if len(cfg.Listeners) != 2 {
		t.Fatalf("Listeners len=%d want 2", len(cfg.Listeners))
	}
	if cfg.Listeners[0].Protocol != "tcp" || cfg.Listeners[0].ListenAddr != ":25565" || cfg.Listeners[0].Upstream != "" {
		t.Fatalf("Listeners[0]=%+v", cfg.Listeners[0])
	}
	if cfg.Listeners[1].Protocol != "udp" || cfg.Listeners[1].ListenAddr != ":19132" || cfg.Listeners[1].Upstream != "127.0.0.1:19132" {
		t.Fatalf("Listeners[1]=%+v", cfg.Listeners[1])
	}
}

func TestFileConfigProvider_Load_UDPListenerRequiresUpstream(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "prism.yaml")

	if err := os.WriteFile(path, []byte(`
listeners:
  - listen_addr: ":19132"
    protocol: "udp"
routes: {}
`), 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	p := NewFileConfigProvider(path)
	_, err := p.Load(context.Background())
	if err == nil {
		t.Fatalf("Load: expected error")
	}
}

func TestFileConfigProvider_Load_TunnelServiceProtoAndRemoteAddr(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "prism.yaml")

	if err := os.WriteFile(path, []byte(`
routes: {}

tunnel:
  client:
    server_addr: "127.0.0.1:7000"
  services:
    - name: "bedrock"
      proto: "udp"
      local_addr: "127.0.0.1:19132"
      remote_addr: ":19132"
`), 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	p := NewFileConfigProvider(path)
	cfg, err := p.Load(context.Background())
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if len(cfg.Tunnel.Services) != 1 {
		t.Fatalf("Tunnel.Services len=%d want 1", len(cfg.Tunnel.Services))
	}
	s := cfg.Tunnel.Services[0]
	if s.Name != "bedrock" || s.Proto != "udp" || s.LocalAddr != "127.0.0.1:19132" || s.RemoteAddr != ":19132" {
		t.Fatalf("Tunnel.Services[0]=%+v", s)
	}
}
