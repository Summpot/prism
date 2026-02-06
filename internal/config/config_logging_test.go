package config

import (
	"context"
	"os"
	"path/filepath"
	"testing"
)

func TestFileConfigProvider_LoggingDefaults(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "config.json")
	if err := os.WriteFile(path, []byte(`{
  "listen_addr": ":25565",
  "admin_addr": ":8080",
  "routes": {}
}`), 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	p := NewFileConfigProvider(path)
	cfg, err := p.Load(context.Background())
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if cfg.Logging.Level != "info" {
		t.Fatalf("level=%q want info", cfg.Logging.Level)
	}
	if cfg.Logging.Format != "json" {
		t.Fatalf("format=%q want json", cfg.Logging.Format)
	}
	if cfg.Logging.Output != "stderr" {
		t.Fatalf("output=%q want stderr", cfg.Logging.Output)
	}
	if cfg.Logging.AdminBuffer.Size != 1000 {
		t.Fatalf("admin_buffer.size=%d want 1000", cfg.Logging.AdminBuffer.Size)
	}
	if cfg.Logging.AdminBuffer.Enabled {
		t.Fatalf("admin_buffer.enabled=true want false")
	}
}

func TestFileConfigProvider_LoggingOverrides(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "config.json")
	if err := os.WriteFile(path, []byte(`{
  "listen_addr": ":25565",
  "admin_addr": ":8080",
  "logging": {
    "level": "debug",
    "format": "text",
    "output": "stdout",
    "add_source": true,
    "admin_buffer": {"enabled": true, "size": 12}
  },
  "routes": {}
}`), 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	p := NewFileConfigProvider(path)
	cfg, err := p.Load(context.Background())
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if cfg.Logging.Level != "debug" {
		t.Fatalf("level=%q want debug", cfg.Logging.Level)
	}
	if cfg.Logging.Format != "text" {
		t.Fatalf("format=%q want text", cfg.Logging.Format)
	}
	if cfg.Logging.Output != "stdout" {
		t.Fatalf("output=%q want stdout", cfg.Logging.Output)
	}
	if !cfg.Logging.AddSource {
		t.Fatalf("add_source=false want true")
	}
	if !cfg.Logging.AdminBuffer.Enabled || cfg.Logging.AdminBuffer.Size != 12 {
		t.Fatalf("admin_buffer=%#v want enabled size=12", cfg.Logging.AdminBuffer)
	}
}
