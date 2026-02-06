package proxy

import (
	"context"
	"net"
	"time"
)

type Dialer interface {
	DialContext(ctx context.Context, network, address string) (net.Conn, error)
}

type NetDialerOptions struct {
	Timeout time.Duration
}

type NetDialer struct {
	d net.Dialer
}

func NewNetDialer(opts *NetDialerOptions) *NetDialer {
	nd := &NetDialer{}
	if opts != nil {
		nd.d.Timeout = opts.Timeout
	}
	return nd
}

func (d *NetDialer) DialContext(ctx context.Context, network, address string) (net.Conn, error) {
	return d.d.DialContext(ctx, network, address)
}
