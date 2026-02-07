package tunnel

import (
	"context"
	"fmt"
	"net"
	"strings"
)

// Transport is the wire transport between prismc and prisms.
//
// It is intentionally small: the tunnel layer only needs a way to accept/dial
// a long-lived connection and then open/accept independent streams on it.
//
// Implementations:
// - tcp: net.Conn + yamux
// - udp: KCP (reliable UDP) + yamux
// - quic: QUIC native streams

type Transport interface {
	Listen(addr string, opts TransportListenOptions) (TransportListener, error)
	Dial(ctx context.Context, addr string, opts TransportDialOptions) (TransportSession, error)
	Name() string
}

type TransportListenOptions struct {
	// QUIC options; ignored by non-QUIC transports.
	QUIC QUICOptions
}

type TransportDialOptions struct {
	// QUIC options; ignored by non-QUIC transports.
	QUIC QUICDialOptions
}

type TransportListener interface {
	Accept(ctx context.Context) (TransportSession, error)
	Close() error
	Addr() net.Addr
}

type TransportSession interface {
	OpenStream(ctx context.Context) (net.Conn, error)
	AcceptStream(ctx context.Context) (net.Conn, error)
	Close() error
	RemoteAddr() net.Addr
	LocalAddr() net.Addr
}

func ParseTransport(name string) (string, error) {
	n := strings.TrimSpace(strings.ToLower(name))
	if n == "" {
		n = "tcp"
	}
	switch n {
	case "tcp", "udp", "quic":
		return n, nil
	default:
		return "", fmt.Errorf("tunnel: unknown transport %q (expected tcp|udp|quic)", name)
	}
}
