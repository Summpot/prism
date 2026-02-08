package config

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestFileConfigProvider_Load_TunnelClientAndServerEnabled(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "prism.yaml")

	if err := os.WriteFile(path, []byte(`
routes: []

tunnel:
  client:
    server_addr: "127.0.0.1:7000"
  services:
    - name: "svc"
      local_addr: "127.0.0.1:25565"
`), 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	p := NewFileConfigProvider(path)
	cfg, err := p.Load(context.Background())
	if err != nil {
		t.Fatalf("Load: %v", err)
	}

	if len(cfg.Listeners) != 0 {
		t.Fatalf("Listeners=%d want 0 (client-only)", len(cfg.Listeners))
	}
	if cfg.Tunnel.Client == nil {
		t.Fatalf("Tunnel.Client=nil want non-nil")
	}
	if cfg.Tunnel.Client.Transport != "tcp" {
		t.Fatalf("Tunnel.Client.Transport=%q want %q", cfg.Tunnel.Client.Transport, "tcp")
	}
	if cfg.Tunnel.Client.ServerAddr != "127.0.0.1:7000" {
		t.Fatalf("Tunnel.Client.ServerAddr=%q", cfg.Tunnel.Client.ServerAddr)
	}
	if cfg.Tunnel.Client.DialTimeout != 5*time.Second {
		t.Fatalf("Tunnel.Client.DialTimeout=%s want %s", cfg.Tunnel.Client.DialTimeout, 5*time.Second)
	}
	if len(cfg.Tunnel.Services) != 1 {
		t.Fatalf("Tunnel.Services len=%d want 1", len(cfg.Tunnel.Services))
	}
	if cfg.Tunnel.Services[0].Name != "svc" || cfg.Tunnel.Services[0].LocalAddr != "127.0.0.1:25565" {
		t.Fatalf("Tunnel.Services[0]=%+v", cfg.Tunnel.Services[0])
	}
}
