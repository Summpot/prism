package proxy

import (
	"context"
	"log/slog"
	"net"
	"sync/atomic"
	"time"

	"prism/internal/config"
)

type ForwardHandlerOptions struct {
	Network  string
	Upstream string
	Dialer   Dialer
	Bridge   *ProxyBridge
	Logger   *slog.Logger
	Timeouts config.Timeouts
}

// ForwardHandler forwards a TCP connection to a fixed upstream.
//
// It is used for port-based forwarding where hostname routing is not needed.
//
//nolint:revive // name is fine.
type ForwardHandler struct {
	v atomic.Value // ForwardHandlerOptions
}

func NewForwardHandler(opts ForwardHandlerOptions) *ForwardHandler {
	h := &ForwardHandler{}
	h.v.Store(opts)
	return h
}

func (h *ForwardHandler) Update(opts ForwardHandlerOptions) {
	h.v.Store(opts)
}

func (h *ForwardHandler) Handle(ctx context.Context, conn net.Conn) {
	optsAny := h.v.Load()
	opts, _ := optsAny.(ForwardHandlerOptions)
	if conn == nil || opts.Dialer == nil || opts.Bridge == nil {
		if conn != nil {
			_ = conn.Close()
		}
		return
	}
	logger := opts.Logger
	if logger == nil {
		logger = slog.Default()
	}
	network := opts.Network
	if network == "" {
		network = "tcp"
	}
	if opts.Timeouts.IdleTimeout > 0 {
		_ = conn.SetDeadline(time.Now().Add(opts.Timeouts.IdleTimeout))
	}

	up, err := opts.Dialer.DialContext(ctx, network, opts.Upstream)
	if err != nil {
		logger.Warn("proxy: forward dial failed", "upstream", opts.Upstream, "err", err)
		_ = conn.Close()
		return
	}

	// No pre-read bytes: initial stream is just the client connection.
	_ = opts.Bridge.Proxy(ctx, conn, up, conn)
}
