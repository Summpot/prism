package app

import (
	"context"
	"errors"
	"net"
	"testing"
	"time"

	"prism/internal/config"
	"prism/internal/proxy"
	"prism/internal/tunnel"
)

type fakeTunnelSession struct {
	remote net.Addr
	local  net.Addr
}

func (s *fakeTunnelSession) OpenStream(context.Context) (net.Conn, error) {
	return nil, errors.New("not implemented")
}
func (s *fakeTunnelSession) AcceptStream(context.Context) (net.Conn, error) {
	return nil, errors.New("not implemented")
}
func (s *fakeTunnelSession) Close() error         { return nil }
func (s *fakeTunnelSession) RemoteAddr() net.Addr { return s.remote }
func (s *fakeTunnelSession) LocalAddr() net.Addr  { return s.local }

func TestTunnelServiceAutoListener_Reconcile_SkipsRouteOnly(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	mgr := tunnel.NewManager(nil)
	sess := &fakeTunnelSession{
		remote: &net.TCPAddr{IP: net.IPv4(127, 0, 0, 1), Port: 11111},
		local:  &net.TCPAddr{IP: net.IPv4(127, 0, 0, 1), Port: 22222},
	}

	// Even if a buggy/old client sets remote_addr, route_only must prevent
	// server-side exposure.
	err := mgr.RegisterClient("c-1", sess, []tunnel.RegisteredService{{
		Name:       "svc",
		Proto:      "tcp",
		LocalAddr:  "127.0.0.1:25565",
		RouteOnly:  true,
		RemoteAddr: "127.0.0.1:0",
	}})
	if err != nil {
		t.Fatalf("RegisterClient: %v", err)
	}

	a := newTunnelServiceAutoListener(ctx, mgr, nil, nil)
	a.UpdateRuntime(proxy.NewNetDialer(nil), proxy.NewProxyBridge(proxy.ProxyBridgeOptions{}), config.Timeouts{IdleTimeout: 0})
	a.Reconcile()

	if got := len(a.running); got != 0 {
		t.Fatalf("running len=%d want 0", got)
	}
}

func TestTunnelServiceAutoListener_Reconcile_StartsRemoteListener(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	mgr := tunnel.NewManager(nil)
	sess := &fakeTunnelSession{
		remote: &net.TCPAddr{IP: net.IPv4(127, 0, 0, 1), Port: 11111},
		local:  &net.TCPAddr{IP: net.IPv4(127, 0, 0, 1), Port: 22222},
	}

	err := mgr.RegisterClient("c-1", sess, []tunnel.RegisteredService{{
		Name:       "svc",
		Proto:      "tcp",
		LocalAddr:  "127.0.0.1:25565",
		RemoteAddr: "127.0.0.1:0",
	}})
	if err != nil {
		t.Fatalf("RegisterClient: %v", err)
	}

	a := newTunnelServiceAutoListener(ctx, mgr, nil, nil)
	a.UpdateRuntime(proxy.NewNetDialer(nil), proxy.NewProxyBridge(proxy.ProxyBridgeOptions{}), config.Timeouts{IdleTimeout: 0})
	a.Reconcile()

	deadline := time.Now().Add(500 * time.Millisecond)
	for len(a.running) != 1 {
		if time.Now().After(deadline) {
			break
		}
		time.Sleep(10 * time.Millisecond)
	}
	if got := len(a.running); got != 1 {
		t.Fatalf("running len=%d want 1", got)
	}

	a.ShutdownAll(context.Background())
}
