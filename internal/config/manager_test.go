package config

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestManager_ReloadsOnFileChange(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "config.yaml")

	write := func(body string) {
		if err := os.WriteFile(path, []byte(body), 0o600); err != nil {
			t.Fatalf("WriteFile: %v", err)
		}
		// Ensure modtime advances on filesystems with coarse timestamps.
		time.Sleep(15 * time.Millisecond)
	}

	write(`
listeners:
  - listen_addr: ":25565"
    protocol: "tcp"
routes: {"a.example.com": "127.0.0.1:1"}
`)

	p := NewFileConfigProvider(path)
	m := NewManager(p, ManagerOptions{PollInterval: 10 * time.Millisecond})

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	if _, err := m.LoadInitial(ctx); err != nil {
		t.Fatalf("LoadInitial: %v", err)
	}

	changedCh := make(chan *Config, 1)
	m.Subscribe(func(_ *Config, newCfg *Config) {
		select {
		case changedCh <- newCfg:
		default:
		}
	})
	m.Start(ctx)

	write(`
listeners:
  - listen_addr: ":25565"
    protocol: "tcp"
routes: {"b.example.com": "127.0.0.1:2"}
`)

	select {
	case cfg := <-changedCh:
		if _, ok := cfg.Routes["b.example.com"]; !ok {
			t.Fatalf("expected updated routes, got: %#v", cfg.Routes)
		}
	case <-ctx.Done():
		t.Fatalf("timed out waiting for reload")
	}
}
