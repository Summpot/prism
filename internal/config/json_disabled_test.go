package config

import (
	"context"
	"os"
	"path/filepath"
	"testing"
)

func TestFileConfigProvider_RejectsJSON(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "prism.json")
	if err := os.WriteFile(path, []byte(`{"listen_addr":":25565","routes":{}}`), 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	p := NewFileConfigProvider(path)
	if _, err := p.Load(context.Background()); err == nil {
		t.Fatalf("expected error, got nil")
	}
}
