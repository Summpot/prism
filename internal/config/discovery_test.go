package config

import (
	"os"
	"path/filepath"
	"testing"
)

func TestDiscoverConfigPath_PriorityOrder(t *testing.T) {
	tmp := t.TempDir()

	write := func(name string) {
		p := filepath.Join(tmp, name)
		if err := os.WriteFile(p, []byte("# test\n"), 0o600); err != nil {
			t.Fatalf("WriteFile(%s): %v", name, err)
		}
	}

	// If multiple files exist, prism.toml should win.
	write("prism.yml")
	write("prism.yaml")
	write("prism.toml")

	got, err := DiscoverConfigPath(tmp)
	if err != nil {
		t.Fatalf("DiscoverConfigPath: %v", err)
	}
	want := filepath.Join(tmp, "prism.toml")
	if got != want {
		t.Fatalf("path=%q want %q", got, want)
	}
}

func TestDiscoverConfigPath_DoesNotUseLegacyConfigJSON(t *testing.T) {
	tmp := t.TempDir()
	legacy := filepath.Join(tmp, "config.json")
	if err := os.WriteFile(legacy, []byte("{}"), 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	if _, err := DiscoverConfigPath(tmp); err == nil {
		t.Fatalf("expected error, got nil")
	}
}

func TestDiscoverConfigPath_ErrWhenMissing(t *testing.T) {
	tmp := t.TempDir()
	if _, err := DiscoverConfigPath(tmp); err == nil {
		t.Fatalf("expected error, got nil")
	}
}
