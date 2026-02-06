package config

import (
	"context"
	"os"
	"sync"
	"sync/atomic"
	"time"
)

type watchableProvider interface {
	WatchPath() string
}

// Manager provides a zero-downtime config reload loop.
//
// It keeps an atomic snapshot of the latest successfully loaded config.
// Existing sessions should continue using the snapshot they started with;
// new sessions should read the latest snapshot.
//
// Manager only reloads from providers that also implement WatchPath().
// (e.g. FileConfigProvider)
type Manager struct {
	provider ConfigProvider

	pollInterval time.Duration
	watchPath    string

	lastMu   sync.Mutex
	lastMod  time.Time
	lastSize int64

	v atomic.Value // *Config

	subsMu sync.Mutex
	subs   []func(oldCfg, newCfg *Config)
}

type ManagerOptions struct {
	PollInterval time.Duration
}

func NewManager(provider ConfigProvider, opts ManagerOptions) *Manager {
	m := &Manager{provider: provider}
	m.pollInterval = opts.PollInterval
	if m.pollInterval <= 0 {
		m.pollInterval = 1 * time.Second
	}
	if wp, ok := provider.(watchableProvider); ok {
		m.watchPath = wp.WatchPath()
	}
	return m
}

func (m *Manager) Current() *Config {
	cfg, _ := m.v.Load().(*Config)
	return cfg
}

func (m *Manager) Subscribe(fn func(oldCfg, newCfg *Config)) {
	if fn == nil {
		return
	}
	m.subsMu.Lock()
	m.subs = append(m.subs, fn)
	m.subsMu.Unlock()
}

func (m *Manager) LoadInitial(ctx context.Context) (*Config, error) {
	cfg, err := m.provider.Load(ctx)
	if err != nil {
		return nil, err
	}
	m.SetCurrent(cfg)
	return cfg, nil
}

// SetCurrent seeds or replaces the current config snapshot without calling the provider.
// This is intended for startup wiring where the config may already have been loaded.
func (m *Manager) SetCurrent(cfg *Config) {
	if cfg == nil {
		return
	}
	m.v.Store(cfg)
	_ = m.captureStatLocked()
}

// ReloadNow forces a reload and, if successful, swaps the current snapshot and
// notifies subscribers.
func (m *Manager) ReloadNow(ctx context.Context) error {
	cfg, err := m.provider.Load(ctx)
	if err != nil {
		return err
	}
	old, _ := m.v.Load().(*Config)
	m.v.Store(cfg)
	_ = m.captureStatLocked()
	m.notify(old, cfg)
	return nil
}

func (m *Manager) Start(ctx context.Context) {
	if m.watchPath == "" {
		return
	}
	go m.loop(ctx)
}

func (m *Manager) loop(ctx context.Context) {
	t := time.NewTicker(m.pollInterval)
	defer t.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case <-t.C:
			changed, err := m.changed()
			if err != nil || !changed {
				continue
			}
			_ = m.ReloadNow(ctx) // best-effort; keep previous snapshot on error
		}
	}
}

func (m *Manager) changed() (bool, error) {
	fi, err := os.Stat(m.watchPath)
	if err != nil {
		return false, err
	}

	m.lastMu.Lock()
	defer m.lastMu.Unlock()

	mod := fi.ModTime()
	sz := fi.Size()
	if mod.After(m.lastMod) || sz != m.lastSize {
		m.lastMod = mod
		m.lastSize = sz
		return true, nil
	}
	return false, nil
}

func (m *Manager) captureStatLocked() error {
	if m.watchPath == "" {
		return nil
	}

	fi, err := os.Stat(m.watchPath)
	if err != nil {
		return err
	}
	m.lastMu.Lock()
	m.lastMod = fi.ModTime()
	m.lastSize = fi.Size()
	m.lastMu.Unlock()
	return nil
}

func (m *Manager) notify(oldCfg, newCfg *Config) {
	m.subsMu.Lock()
	subs := append([]func(oldCfg, newCfg *Config){}, m.subs...)
	m.subsMu.Unlock()

	for _, fn := range subs {
		fn(oldCfg, newCfg)
	}
}
