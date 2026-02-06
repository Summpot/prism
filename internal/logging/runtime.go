package logging

import (
	"errors"
	"fmt"
	"io"
	"log/slog"
	"os"
	"path/filepath"
	"strings"

	"prism/internal/config"
)

// Runtime owns the process logger configuration and any associated resources
// (e.g. an output file handle and optional in-memory admin buffer).
//
// Currently, only the log level is considered safely reloadable at runtime.
// Changing output/format/add_source requires a restart.
type Runtime struct {
	logger   *slog.Logger
	levelVar slog.LevelVar

	out    io.Writer
	closer io.Closer
	store  *LineStore

	cfg config.LoggingConfig
}

func NewRuntime(cfg config.LoggingConfig) (*Runtime, error) {
	cfg = normalizeConfig(cfg)

	level, err := parseLevel(cfg.Level)
	if err != nil {
		return nil, err
	}

	r := &Runtime{cfg: cfg}
	r.levelVar.Set(level)

	out, closer, err := openOutput(cfg.Output)
	if err != nil {
		return nil, err
	}
	r.out = out
	r.closer = closer

	var w io.Writer = out
	if cfg.AdminBuffer.Enabled {
		size := cfg.AdminBuffer.Size
		if size <= 0 {
			size = 1000
		}
		r.store = NewLineStore(size)
		w = io.MultiWriter(out, r.store)
	}

	hopts := &slog.HandlerOptions{Level: &r.levelVar, AddSource: cfg.AddSource}
	var h slog.Handler
	switch strings.ToLower(strings.TrimSpace(cfg.Format)) {
	case "text":
		h = slog.NewTextHandler(w, hopts)
	case "json", "":
		h = slog.NewJSONHandler(w, hopts)
	default:
		return nil, fmt.Errorf("logging: unknown format %q", cfg.Format)
	}

	r.logger = slog.New(h).With(
		slog.String("app", "prism"),
	)
	return r, nil
}

func (r *Runtime) Logger() *slog.Logger {
	if r == nil || r.logger == nil {
		return slog.Default()
	}
	return r.logger
}

func (r *Runtime) Store() *LineStore { return r.store }

// Apply updates runtime-reloadable logging settings.
//
// Today this means only the log level.
func (r *Runtime) Apply(cfg config.LoggingConfig) error {
	if r == nil {
		return nil
	}
	cfg = normalizeConfig(cfg)
	lvl, err := parseLevel(cfg.Level)
	if err != nil {
		return err
	}
	r.levelVar.Set(lvl)
	r.cfg.Level = cfg.Level
	return nil
}

// NeedsRestart returns true if applying newCfg would require rebuilding the logger.
func (r *Runtime) NeedsRestart(newCfg config.LoggingConfig) bool {
	if r == nil {
		return false
	}
	newCfg = normalizeConfig(newCfg)
	oldCfg := normalizeConfig(r.cfg)
	return !strings.EqualFold(strings.TrimSpace(oldCfg.Format), strings.TrimSpace(newCfg.Format)) ||
		!strings.EqualFold(strings.TrimSpace(oldCfg.Output), strings.TrimSpace(newCfg.Output)) ||
		oldCfg.AddSource != newCfg.AddSource ||
		oldCfg.AdminBuffer.Enabled != newCfg.AdminBuffer.Enabled ||
		oldCfg.AdminBuffer.Size != newCfg.AdminBuffer.Size
}

func (r *Runtime) Close() error {
	if r == nil || r.closer == nil {
		return nil
	}
	return r.closer.Close()
}

func normalizeConfig(cfg config.LoggingConfig) config.LoggingConfig {
	if strings.TrimSpace(cfg.Level) == "" {
		cfg.Level = "info"
	}
	if strings.TrimSpace(cfg.Format) == "" {
		cfg.Format = "json"
	}
	if strings.TrimSpace(cfg.Output) == "" {
		cfg.Output = "stderr"
	}
	return cfg
}

func parseLevel(s string) (slog.Level, error) {
	s = strings.TrimSpace(strings.ToLower(s))
	switch s {
	case "debug":
		return slog.LevelDebug, nil
	case "info", "":
		return slog.LevelInfo, nil
	case "warn", "warning":
		return slog.LevelWarn, nil
	case "error":
		return slog.LevelError, nil
	default:
		return slog.LevelInfo, fmt.Errorf("logging: unknown level %q", s)
	}
}

func openOutput(output string) (io.Writer, io.Closer, error) {
	o := strings.TrimSpace(output)
	switch strings.ToLower(o) {
	case "stderr", "":
		return os.Stderr, nil, nil
	case "stdout":
		return os.Stdout, nil, nil
	case "discard", "none", "null":
		return io.Discard, nil, nil
	default:
		// Treat as a file path.
		path := filepath.Clean(o)
		f, err := os.OpenFile(path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o644)
		if err != nil {
			return nil, nil, fmt.Errorf("logging: open %s: %w", path, err)
		}
		return f, f, nil
	}
}

var ErrRestartRequired = errors.New("logging: restart required")
