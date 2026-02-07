package app

import (
	"context"
	"log/slog"
	"strings"
	"sync"
	"time"

	"prism/internal/config"
	"prism/internal/proxy"
	"prism/internal/server"
	"prism/internal/tunnel"
)

type tunnelServiceAutoListener struct {
	ctx    context.Context
	logger *slog.Logger
	tm     *tunnel.Manager
	metrics any

	runtimeMu sync.RWMutex
	dialer    proxy.Dialer
	bridge    *proxy.ProxyBridge
	timeouts  config.Timeouts

	mu      sync.Mutex
	running map[string]*tunnelServiceListener
}

type tunnelServiceListener struct {
	name string
	proto string
	addr  string

	forward *proxy.ForwardHandler
	udpFwd  *proxy.UDPForwarder

	tcp *server.TCPServer
	udp *server.UDPServer
}

func newTunnelServiceAutoListener(ctx context.Context, tm *tunnel.Manager, metrics any, logger *slog.Logger) *tunnelServiceAutoListener {
	if logger == nil {
		logger = slog.Default()
	}
	return &tunnelServiceAutoListener{ctx: ctx, tm: tm, metrics: metrics, logger: logger, running: map[string]*tunnelServiceListener{}}
}

func (a *tunnelServiceAutoListener) UpdateRuntime(dialer proxy.Dialer, bridge *proxy.ProxyBridge, timeouts config.Timeouts) {
	a.runtimeMu.Lock()
	a.dialer = dialer
	a.bridge = bridge
	a.timeouts = timeouts
	a.runtimeMu.Unlock()

	// Update existing handlers so new sessions use current dialer/bridge/timeouts.
	a.mu.Lock()
	defer a.mu.Unlock()
	for _, r := range a.running {
		if r == nil {
			continue
		}
		if r.forward != nil {
			r.forward.Update(proxy.ForwardHandlerOptions{
				Network:  "tcp",
				Upstream: "tunnel:" + r.name,
				Dialer:   dialer,
				Bridge:   bridge,
				Logger:   a.logger,
				Timeouts: timeouts,
			})
		}
		if r.udpFwd != nil {
			r.udpFwd.Update(proxy.UDPForwarderOptions{
				Upstream:    "tunnel:" + r.name,
				Dialer:      dialer,
				IdleTimeout: timeouts.IdleTimeout,
				Logger:      a.logger,
			})
		}
	}
}

func (a *tunnelServiceAutoListener) Reconcile() {
	if a.tm == nil {
		return
	}

	snaps := a.tm.SnapshotServices()
	desired := map[string]tunnel.RegisteredService{}
	for _, s := range snaps {
		svc := s.Service
		name := strings.TrimSpace(svc.Name)
		if name == "" {
			continue
		}
		proto := strings.TrimSpace(strings.ToLower(svc.Proto))
		if proto == "" {
			proto = "tcp"
		}
		remote := strings.TrimSpace(svc.RemoteAddr)
		if remote == "" {
			continue
		}
		svc.Proto = proto
		svc.RemoteAddr = remote
		desired[name] = svc
	}

	a.mu.Lock()
	defer a.mu.Unlock()

	// Stop removed or changed listeners.
	for name, r := range a.running {
		svc, ok := desired[name]
		if !ok {
			a.shutdownLocked(name, r)
			delete(a.running, name)
			continue
		}
		if r == nil {
			delete(a.running, name)
			continue
		}
		if r.proto != svc.Proto || r.addr != svc.RemoteAddr {
			a.shutdownLocked(name, r)
			delete(a.running, name)
		}
	}

	// Start new listeners.
	for name, svc := range desired {
		if a.running[name] != nil {
			continue
		}

		a.runtimeMu.RLock()
		dialer := a.dialer
		bridge := a.bridge
		timeouts := a.timeouts
		a.runtimeMu.RUnlock()
		if dialer == nil || bridge == nil {
			// Not ready yet; will be retried on next reconcile.
			continue
		}

		rl := &tunnelServiceListener{name: name, proto: svc.Proto, addr: svc.RemoteAddr}
		switch svc.Proto {
		case "tcp":
			rl.forward = proxy.NewForwardHandler(proxy.ForwardHandlerOptions{
				Network:  "tcp",
				Upstream: "tunnel:" + name,
				Dialer:   dialer,
				Bridge:   bridge,
				Logger:   a.logger,
				Timeouts: timeouts,
			})
			rl.tcp = server.NewTCPServer(svc.RemoteAddr, rl.forward, a.metrics, a.logger)
			go func(s *server.TCPServer, svcName string, addr string) {
				if err := s.ListenAndServe(a.ctx); err != nil {
					// Avoid tearing down Prism on per-service listener failures.
					a.logger.Warn("tunnel: service tcp listener stopped", "service", svcName, "addr", addr, "err", err)
				}
			}(rl.tcp, name, svc.RemoteAddr)
		case "udp":
			rl.udpFwd = proxy.NewUDPForwarder(proxy.UDPForwarderOptions{
				Upstream:    "tunnel:" + name,
				Dialer:      dialer,
				IdleTimeout: timeouts.IdleTimeout,
				Logger:      a.logger,
			})
			rl.udp = server.NewUDPServer(svc.RemoteAddr, rl.udpFwd, a.logger)
			go func(s *server.UDPServer, svcName string, addr string) {
				if err := s.ListenAndServe(a.ctx); err != nil {
					a.logger.Warn("tunnel: service udp listener stopped", "service", svcName, "addr", addr, "err", err)
				}
			}(rl.udp, name, svc.RemoteAddr)
		default:
			continue
		}

		a.logger.Info("tunnel: auto listening for service", "service", name, "proto", svc.Proto, "addr", svc.RemoteAddr)
		a.running[name] = rl
	}
}

func (a *tunnelServiceAutoListener) ShutdownAll(ctx context.Context) {
	a.mu.Lock()
	defer a.mu.Unlock()
	for name, r := range a.running {
		a.shutdownLocked(name, r)
		delete(a.running, name)
	}
}

func (a *tunnelServiceAutoListener) shutdownLocked(name string, r *tunnelServiceListener) {
	if r == nil {
		return
	}
	// Best effort; use small timeout.
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	if r.tcp != nil {
		_ = r.tcp.Shutdown(ctx)
	}
	if r.udp != nil {
		_ = r.udp.Shutdown(ctx)
	}
	a.logger.Info("tunnel: stopped service listener", "service", name)
}
