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
	path := filepath.Join(tmp, "prism.json")

	if err := os.WriteFile(path, []byte(`{
  "server_enabled": false,
  "listen_addr": ":25565",
  "admin_addr": ":8080",
  "routes": {},
  "tunnel_client": {
    "enabled": true,
    "server_addr": "127.0.0.1:7000",
    "services": [
      {"name": "svc", "local_addr": "127.0.0.1:25565"}
    ]
  }
}`), 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	p := NewFileConfigProvider(path)
	cfg, err := p.Load(context.Background())
	if err != nil {
		t.Fatalf("Load: %v", err)
	}

	if cfg.ServerEnabled {
		t.Fatalf("ServerEnabled=true want false")
	}
	if !cfg.TunnelClient.Enabled {
		t.Fatalf("TunnelClient.Enabled=false want true")
	}
	if cfg.TunnelClient.Transport != "tcp" {
		t.Fatalf("TunnelClient.Transport=%q want %q", cfg.TunnelClient.Transport, "tcp")
	}
	if cfg.TunnelClient.ServerAddr != "127.0.0.1:7000" {
		t.Fatalf("TunnelClient.ServerAddr=%q", cfg.TunnelClient.ServerAddr)
	}
	if cfg.TunnelClient.DialTimeout != 5*time.Second {
		t.Fatalf("TunnelClient.DialTimeout=%s want %s", cfg.TunnelClient.DialTimeout, 5*time.Second)
	}
	if len(cfg.TunnelClient.Services) != 1 {
		t.Fatalf("TunnelClient.Services len=%d want 1", len(cfg.TunnelClient.Services))
	}
	if cfg.TunnelClient.Services[0].Name != "svc" || cfg.TunnelClient.Services[0].LocalAddr != "127.0.0.1:25565" {
		t.Fatalf("TunnelClient.Services[0]=%+v", cfg.TunnelClient.Services[0])
	}
}
