package proxy

import (
	"context"
	"fmt"
	"net"
	"strings"

	"prism/internal/tunnel"
)

// TunnelDialer supports upstream targets of the form:
//
//	tunnel:<service>
//	tunnel://<service>
//
// Everything else is delegated to Fallback.
//
// The tunnel manager is owned by the prisms process and is populated by
// prismc registrations.
type TunnelDialer struct {
	Fallback Dialer
	Tunnel   *tunnel.Manager
}

func NewTunnelDialer(fallback Dialer, tm *tunnel.Manager) *TunnelDialer {
	return &TunnelDialer{Fallback: fallback, Tunnel: tm}
}

func (d *TunnelDialer) DialContext(ctx context.Context, network, address string) (net.Conn, error) {
	if svc, ok := parseTunnelAddress(address); ok {
		if d.Tunnel == nil {
			return nil, fmt.Errorf("proxy: tunnel dial requested but tunnel is not configured")
		}
		n := strings.ToLower(strings.TrimSpace(network))
		if strings.HasPrefix(n, "udp") {
			return d.Tunnel.DialServiceUDP(ctx, svc)
		}
		return d.Tunnel.DialService(ctx, svc)
	}
	if d.Fallback == nil {
		return nil, fmt.Errorf("proxy: no dialer configured")
	}
	return d.Fallback.DialContext(ctx, network, address)
}

func parseTunnelAddress(addr string) (service string, ok bool) {
	a := strings.TrimSpace(addr)
	if a == "" {
		return "", false
	}
	if strings.HasPrefix(strings.ToLower(a), "tunnel://") {
		service = strings.TrimSpace(a[len("tunnel://"):])
		return service, service != ""
	}
	if strings.HasPrefix(strings.ToLower(a), "tunnel:") {
		service = strings.TrimSpace(a[len("tunnel:"):])
		return service, service != ""
	}
	return "", false
}

var _ Dialer = (*TunnelDialer)(nil)
