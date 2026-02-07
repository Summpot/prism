package config

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestResolveConfigPath_FlagOverridesEnv(t *testing.T) {
	t.Setenv(EnvConfigPath, filepath.Join(t.TempDir(), "from-env.toml"))

	res, err := ResolveConfigPath("from-flag.toml")
	if err != nil {
		t.Fatalf("ResolveConfigPath: %v", err)
	}
	if res.Source != ConfigPathSourceFlag {
		t.Fatalf("source=%q want %q", res.Source, ConfigPathSourceFlag)
	}
	if res.Path != filepath.Clean("from-flag.toml") {
		t.Fatalf("path=%q want %q", res.Path, filepath.Clean("from-flag.toml"))
	}
}

func TestResolveConfigPath_EnvUsedWhenNoFlag(t *testing.T) {
	p := filepath.Join(t.TempDir(), "cfg.yaml")
	t.Setenv(EnvConfigPath, p)

	res, err := ResolveConfigPath("")
	if err != nil {
		t.Fatalf("ResolveConfigPath: %v", err)
	}
	if res.Source != ConfigPathSourceEnv {
		t.Fatalf("source=%q want %q", res.Source, ConfigPathSourceEnv)
	}
	if res.Path != filepath.Clean(p) {
		t.Fatalf("path=%q want %q", res.Path, filepath.Clean(p))
	}
}

func TestResolveConfigPath_EnvDirectoryDiscoversConfig(t *testing.T) {
	dir := t.TempDir()
	cfg := filepath.Join(dir, "prism.yml")
	if err := os.WriteFile(cfg, []byte("# test\n"), 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	t.Setenv(EnvConfigPath, dir)

	res, err := ResolveConfigPath("")
	if err != nil {
		t.Fatalf("ResolveConfigPath: %v", err)
	}
	if res.Source != ConfigPathSourceEnv {
		t.Fatalf("source=%q want %q", res.Source, ConfigPathSourceEnv)
	}
	if res.Path != cfg {
		t.Fatalf("path=%q want %q", res.Path, cfg)
	}
}

func TestResolveConfigPath_CWDDiscoveryWhenPresent(t *testing.T) {
	oldwd, err := os.Getwd()
	if err != nil {
		t.Fatalf("Getwd: %v", err)
	}
	defer func() { _ = os.Chdir(oldwd) }()

	tmp := t.TempDir()
	if err := os.Chdir(tmp); err != nil {
		t.Fatalf("Chdir: %v", err)
	}
	// Ensure env isn't set.
	t.Setenv(EnvConfigPath, "")

	cfg := filepath.Join(tmp, "prism.toml")
	if err := os.WriteFile(cfg, []byte("# test\n"), 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	res, err := ResolveConfigPath("")
	if err != nil {
		t.Fatalf("ResolveConfigPath: %v", err)
	}
	if res.Source != ConfigPathSourceCWD {
		t.Fatalf("source=%q want %q", res.Source, ConfigPathSourceCWD)
	}
	// DiscoverConfigPath(".") returns a path relative to the working directory.
	if res.Path != filepath.Clean("prism.toml") {
		t.Fatalf("path=%q want %q", res.Path, filepath.Clean("prism.toml"))
	}
}

func TestResolveConfigPath_DefaultWhenNoneFound(t *testing.T) {
	oldwd, err := os.Getwd()
	if err != nil {
		t.Fatalf("Getwd: %v", err)
	}
	defer func() { _ = os.Chdir(oldwd) }()

	if err := os.Chdir(t.TempDir()); err != nil {
		t.Fatalf("Chdir: %v", err)
	}
	t.Setenv(EnvConfigPath, "")

	res, err := ResolveConfigPath("")
	if err != nil {
		t.Fatalf("ResolveConfigPath: %v", err)
	}
	if res.Source != ConfigPathSourceDefault {
		t.Fatalf("source=%q want %q", res.Source, ConfigPathSourceDefault)
	}

	dir, err := os.UserConfigDir()
	if err != nil {
		t.Fatalf("UserConfigDir: %v", err)
	}
	want := filepath.Join(dir, "prism", "prism.toml")
	if res.Path != want {
		t.Fatalf("path=%q want %q", res.Path, want)
	}
}

func TestEnsureConfigFile_CreatesFileOnce(t *testing.T) {
	p := filepath.Join(t.TempDir(), "nested", "prism.toml")

	created, err := EnsureConfigFile(p)
	if err != nil {
		t.Fatalf("EnsureConfigFile: %v", err)
	}
	if !created {
		t.Fatalf("created=false want true")
	}
	b, err := os.ReadFile(p)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if !strings.Contains(string(b), "Prism configuration") {
		t.Fatalf("template did not look like prism config")
	}
	if !strings.Contains(string(b), "[tunnel]") {
		t.Fatalf("expected runnable template to include tunnel config")
	}
	if !strings.Contains(string(b), "[[tunnel.endpoints]]") {
		t.Fatalf("expected runnable template to include tunnel endpoints")
	}

	// The generated config should be parseable.
	prov := NewFileConfigProvider(p)
	cfg, err := prov.Load(context.Background())
	if err != nil {
		t.Fatalf("Load(generated): %v", err)
	}
	if cfg == nil || len(cfg.Tunnel.Listeners) == 0 {
		t.Fatalf("expected generated config to enable at least one tunnel endpoint")
	}

	created2, err := EnsureConfigFile(p)
	if err != nil {
		t.Fatalf("EnsureConfigFile(2): %v", err)
	}
	if created2 {
		t.Fatalf("created=true want false")
	}
}

func TestEnsureConfigFile_UsesTemplateByExtension(t *testing.T) {
	p := filepath.Join(t.TempDir(), "prism.yaml")

	created, err := EnsureConfigFile(p)
	if err != nil {
		t.Fatalf("EnsureConfigFile: %v", err)
	}
	if !created {
		t.Fatalf("created=false want true")
	}
	b, err := os.ReadFile(p)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if !strings.Contains(string(b), "tunnel:") {
		t.Fatalf("expected YAML template to mention tunnel")
	}
	if !strings.Contains(string(b), "endpoints:") {
		t.Fatalf("expected YAML runnable template to include endpoints")
	}

	prov := NewFileConfigProvider(p)
	cfg, err := prov.Load(context.Background())
	if err != nil {
		t.Fatalf("Load(generated yaml): %v", err)
	}
	if cfg == nil || len(cfg.Tunnel.Listeners) == 0 {
		t.Fatalf("expected generated yaml config to enable at least one tunnel endpoint")
	}
}
