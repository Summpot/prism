package config

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestFileConfigProvider_Load_TunnelServices_RouteOnlyRejectsRemoteAddr(t *testing.T) {
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
      route_only: true
      remote_addr: ":25565"
`), 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	p := NewFileConfigProvider(path)
	_, err := p.Load(context.Background())
	if err == nil {
		t.Fatalf("Load: expected error")
	}
	if !strings.Contains(err.Error(), "route_only=true") || !strings.Contains(err.Error(), "remote_addr") {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestFileConfigProvider_Load_TunnelServices_RouteOnlyNormalizesRemoteAddrEmpty(t *testing.T) {
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
      route_only: true
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
	if !cfg.Tunnel.Services[0].RouteOnly {
		t.Fatalf("RouteOnly=false want true")
	}
	if cfg.Tunnel.Services[0].RemoteAddr != "" {
		t.Fatalf("RemoteAddr=%q want empty", cfg.Tunnel.Services[0].RemoteAddr)
	}
}
