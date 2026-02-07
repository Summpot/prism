package config

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestTunnelEndpoints_AcceptsPreferredKey(t *testing.T) {
	p := filepath.Join(t.TempDir(), "prism.yaml")
	data := []byte(`admin_addr: ":8080"

tunnel:
  endpoints:
    - listen_addr: ":7000"
      transport: "tcp"
`)
	if err := os.WriteFile(p, data, 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	cfg, err := NewFileConfigProvider(p).Load(context.Background())
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if len(cfg.Tunnel.Listeners) != 1 {
		t.Fatalf("tunnel.listeners=%d want 1", len(cfg.Tunnel.Listeners))
	}
	if cfg.Tunnel.Listeners[0].ListenAddr != ":7000" {
		t.Fatalf("listen_addr=%q want %q", cfg.Tunnel.Listeners[0].ListenAddr, ":7000")
	}
}

func TestTunnelEndpoints_RejectsLegacyListenersKey(t *testing.T) {
	p := filepath.Join(t.TempDir(), "prism.yaml")
	data := []byte(`admin_addr: ":8080"

tunnel:
  listeners:
    - listen_addr: ":7000"
      transport: "tcp"
`)
	if err := os.WriteFile(p, data, 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	_, err := NewFileConfigProvider(p).Load(context.Background())
	if err == nil {
		t.Fatalf("Load: expected error")
	}
	if !strings.Contains(err.Error(), "listeners") {
		t.Fatalf("unexpected error: %v", err)
	}
}
