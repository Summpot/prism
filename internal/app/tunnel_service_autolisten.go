package app

import (
	"context"
	"log/slog"
	"net"
	"strings"
	"sync"
	"time"

	"prism/internal/config"
	"prism/internal/proxy"
	"prism/internal/server"
	"prism/internal/tunnel"
)

type tunnelServiceAutoListener struct {
	ctx     context.Context
	logger  *slog.Logger
	tm      *tunnel.Manager
	metrics any

	runtimeMu sync.RWMutex
	bridge    *proxy.ProxyBridge
	timeouts  config.Timeouts

	mu      sync.Mutex
	running map[string]*tunnelServiceListener
}

type tunnelServiceListener struct {
	key      string
	clientID string
	name     string
	proto    string
	addr     string

	forward *proxy.ForwardHandler
	udpFwd  *proxy.UDPForwarder

	tcp *server.TCPServer
	udp *server.UDPServer
}

type pinnedTunnelDialer struct {
	tm       *tunnel.Manager
	clientID string
	service  string
}

func (d pinnedTunnelDialer) DialContext(ctx context.Context, network, _ string) (net.Conn, error) {
	n := strings.ToLower(strings.TrimSpace(network))
	if strings.HasPrefix(n, "udp") {
		return d.tm.DialServiceUDPFromClient(ctx, d.clientID, d.service)
	}
	return d.tm.DialServiceFromClient(ctx, d.clientID, d.service)
}

func newTunnelServiceAutoListener(ctx context.Context, tm *tunnel.Manager, metrics any, logger *slog.Logger) *tunnelServiceAutoListener {
	if logger == nil {
		logger = slog.Default()
	}
	return &tunnelServiceAutoListener{ctx: ctx, tm: tm, metrics: metrics, logger: logger, running: map[string]*tunnelServiceListener{}}
}

func (a *tunnelServiceAutoListener) UpdateRuntime(dialer proxy.Dialer, bridge *proxy.ProxyBridge, timeouts config.Timeouts) {
	a.runtimeMu.Lock()
	_ = dialer // auto-listen uses pinned dialers per service; keep param for call-site simplicity.
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
		d := pinnedTunnelDialer{tm: a.tm, clientID: r.clientID, service: r.name}
		if r.forward != nil {
			r.forward.Update(proxy.ForwardHandlerOptions{
				Network:  "tcp",
				Upstream: "tunnel:" + r.name,
				Dialer:   d,
				Bridge:   bridge,
				Logger:   a.logger,
				Timeouts: timeouts,
			})
		}
		if r.udpFwd != nil {
			r.udpFwd.Update(proxy.UDPForwarderOptions{
				Upstream:    "tunnel:" + r.name,
				Dialer:      d,
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
	type desiredSvc struct {
		ClientID string
		Name     string
		Proto    string
		Addr     string
	}
	desired := map[string]desiredSvc{}
	for _, s := range snaps {
		svc := s.Service
		name := strings.TrimSpace(svc.Name)
		if name == "" {
			continue
		}
		if svc.RouteOnly {
			// Route-only services are meant to be referenced via tunnel:<service>
			// (routes/forwards) but never auto-exposed as server-side listeners.
			continue
		}
		cid := strings.TrimSpace(s.ClientID)
		if cid == "" {
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
		key := cid + "/" + name
		desired[key] = desiredSvc{ClientID: cid, Name: name, Proto: proto, Addr: remote}
	}

	a.mu.Lock()
	defer a.mu.Unlock()

	// Stop removed or changed listeners.
	for key, r := range a.running {
		svc, ok := desired[key]
		if !ok {
			a.shutdownLocked(key, r)
			delete(a.running, key)
			continue
		}
		if r == nil {
			delete(a.running, key)
			continue
		}
		if r.clientID != svc.ClientID || r.name != svc.Name || r.proto != svc.Proto || r.addr != svc.Addr {
			a.shutdownLocked(key, r)
			delete(a.running, key)
		}
	}

	// Start new listeners.
	for key, svc := range desired {
		if a.running[key] != nil {
			continue
		}

		a.runtimeMu.RLock()
		bridge := a.bridge
		timeouts := a.timeouts
		a.runtimeMu.RUnlock()
		if bridge == nil {
			// Not ready yet; will be retried on next reconcile.
			continue
		}

		d := pinnedTunnelDialer{tm: a.tm, clientID: svc.ClientID, service: svc.Name}
		rl := &tunnelServiceListener{key: key, clientID: svc.ClientID, name: svc.Name, proto: svc.Proto, addr: svc.Addr}
		switch svc.Proto {
		case "tcp":
			rl.forward = proxy.NewForwardHandler(proxy.ForwardHandlerOptions{
				Network:  "tcp",
				Upstream: "tunnel:" + svc.Name,
				Dialer:   d,
				Bridge:   bridge,
				Logger:   a.logger,
				Timeouts: timeouts,
			})
			rl.tcp = server.NewTCPServer(svc.Addr, rl.forward, a.metrics, a.logger)
			go func(s *server.TCPServer, svcName string, cid string, addr string) {
				if err := s.ListenAndServe(a.ctx); err != nil {
					// Avoid tearing down Prism on per-service listener failures.
					a.logger.Warn("tunnel: service tcp listener stopped", "service", svcName, "cid", cid, "addr", addr, "err", err)
				}
			}(rl.tcp, svc.Name, svc.ClientID, svc.Addr)
		case "udp":
			rl.udpFwd = proxy.NewUDPForwarder(proxy.UDPForwarderOptions{
				Upstream:    "tunnel:" + svc.Name,
				Dialer:      d,
				IdleTimeout: timeouts.IdleTimeout,
				Logger:      a.logger,
			})
			rl.udp = server.NewUDPServer(svc.Addr, rl.udpFwd, a.logger)
			go func(s *server.UDPServer, svcName string, cid string, addr string) {
				if err := s.ListenAndServe(a.ctx); err != nil {
					a.logger.Warn("tunnel: service udp listener stopped", "service", svcName, "cid", cid, "addr", addr, "err", err)
				}
			}(rl.udp, svc.Name, svc.ClientID, svc.Addr)
		default:
			continue
		}

		a.logger.Info("tunnel: auto listening for service", "service", svc.Name, "cid", svc.ClientID, "proto", svc.Proto, "addr", svc.Addr)
		a.running[key] = rl
	}
}

func (a *tunnelServiceAutoListener) ShutdownAll(ctx context.Context) {
	a.mu.Lock()
	defer a.mu.Unlock()
	for key, r := range a.running {
		a.shutdownLocked(key, r)
		delete(a.running, key)
	}
}

func (a *tunnelServiceAutoListener) shutdownLocked(key string, r *tunnelServiceListener) {
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
	a.logger.Info("tunnel: stopped service listener", "service", r.name, "cid", r.clientID, "key", key)
}
