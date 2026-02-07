package tunnel

import (
	"context"
	"errors"
	"net"
	"testing"
)

type fakeSess struct{}

func (s *fakeSess) OpenStream(context.Context) (net.Conn, error) {
	return nil, errors.New("not implemented")
}
func (s *fakeSess) AcceptStream(context.Context) (net.Conn, error) {
	return nil, errors.New("not implemented")
}
func (s *fakeSess) Close() error         { return nil }
func (s *fakeSess) RemoteAddr() net.Addr { return &net.TCPAddr{IP: net.IPv4(127, 0, 0, 1), Port: 1} }
func (s *fakeSess) LocalAddr() net.Addr  { return &net.TCPAddr{IP: net.IPv4(127, 0, 0, 1), Port: 2} }

func TestManager_DuplicateServiceDoesNotOverrideRoutingPrimary(t *testing.T) {
	m := NewManager(nil)

	if err := m.RegisterClient("c1", &fakeSess{}, []RegisteredService{{Name: "svc", Proto: "tcp", LocalAddr: "127.0.0.1:1"}}); err != nil {
		t.Fatalf("RegisterClient c1: %v", err)
	}
	if got := m.primary["svc"]; got != "c1" {
		t.Fatalf("primary[svc]=%q want %q", got, "c1")
	}

	if err := m.RegisterClient("c2", &fakeSess{}, []RegisteredService{{Name: "svc", Proto: "tcp", LocalAddr: "127.0.0.1:2"}}); err != nil {
		t.Fatalf("RegisterClient c2: %v", err)
	}
	// Routing primary should not change.
	if got := m.primary["svc"]; got != "c1" {
		t.Fatalf("primary[svc]=%q want %q", got, "c1")
	}

	snaps := m.SnapshotServices()
	if len(snaps) != 2 {
		t.Fatalf("SnapshotServices len=%d want 2", len(snaps))
	}

	m.UnregisterClient("c1")
	// When the primary disconnects, a remaining registrant should be promoted.
	if got := m.primary["svc"]; got != "c2" {
		t.Fatalf("primary[svc]=%q want %q", got, "c2")
	}
}
