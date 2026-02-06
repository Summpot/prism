package proxy

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"net"
	"sync/atomic"
	"time"

	"prism/internal/config"
	"prism/internal/protocol"
	"prism/internal/router"
)

type SessionHandlerOptions struct {
	Parser   protocol.HostParser
	Resolver router.UpstreamResolver
	Dialer   Dialer
	Bridge   *ProxyBridge
	Metrics  interface {
		IncActive()
		DecActive()
		AddRouteHit(host string)
	}
	Sessions       *SessionRegistry
	Timeouts       config.Timeouts
	MaxHeaderBytes int
}

type SessionHandler struct {
	v atomic.Value // SessionHandlerOptions
}

func NewSessionHandler(opts SessionHandlerOptions) *SessionHandler {
	h := &SessionHandler{}
	h.v.Store(opts)
	return h
}

func (h *SessionHandler) Update(opts SessionHandlerOptions) {
	h.v.Store(opts)
}

func (h *SessionHandler) Handle(ctx context.Context, conn net.Conn) {
	optsAny := h.v.Load()
	opts, _ := optsAny.(SessionHandlerOptions)
	if opts.Parser == nil || opts.Resolver == nil || opts.Dialer == nil || opts.Bridge == nil {
		_ = conn.Close()
		return
	}

	if opts.Metrics != nil {
		opts.Metrics.IncActive()
		defer opts.Metrics.DecActive()
	}
	defer conn.Close()

	maxHeader := opts.MaxHeaderBytes
	if maxHeader <= 0 {
		maxHeader = 64 * 1024
	}

	// Apply handshake timeout via deadline so conn.Read unblocks deterministically.
	if opts.Timeouts.HandshakeTimeout > 0 {
		_ = conn.SetReadDeadline(time.Now().Add(opts.Timeouts.HandshakeTimeout))
	}

	// Capture the initial bytes so we can forward them upstream unchanged.
	captured := make([]byte, 0, min(4096, maxHeader))
	tmp := make([]byte, 4096)

	host := ""
	for len(captured) < maxHeader {
		n, err := conn.Read(tmp)
		if n > 0 {
			need := n
			if len(captured)+need > maxHeader {
				need = maxHeader - len(captured)
			}
			captured = append(captured, tmp[:need]...)
		}
		if err != nil {
			return
		}

		parsedHost, perr := opts.Parser.Parse(captured)
		if perr == nil {
			host = parsedHost
			break
		}
		if errors.Is(perr, protocol.ErrNeedMoreData) {
			continue
		}
		// No match or fatal error: drop the connection (can't route).
		return
	}
	if host == "" {
		return
	}

	// Clear handshake read deadline; the idle timeout (if set) applies from here.
	_ = conn.SetReadDeadline(time.Time{})

	upstreamAddr, ok := opts.Resolver.Resolve(host)
	if !ok {
		return
	}
	if opts.Metrics != nil {
		opts.Metrics.AddRouteHit(host)
	}

	if opts.Timeouts.IdleTimeout > 0 {
		_ = conn.SetDeadline(time.Now().Add(opts.Timeouts.IdleTimeout))
	}

	up, err := opts.Dialer.DialContext(ctx, "tcp", upstreamAddr)
	if err != nil {
		return
	}

	sid := newSessionID()
	if opts.Sessions != nil {
		opts.Sessions.Add(SessionInfo{
			ID:        sid,
			Client:    conn.RemoteAddr().String(),
			Host:      host,
			Upstream:  upstreamAddr,
			StartedAt: time.Now(),
		})
		defer opts.Sessions.Remove(sid)
	}

	initial := io.MultiReader(bytes.NewReader(captured), conn)
	_ = opts.Bridge.Proxy(ctx, conn, up, initial)
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}

func newSessionID() string {
	var b [12]byte
	if _, err := rand.Read(b[:]); err != nil {
		return fmt.Sprintf("%d", time.Now().UnixNano())
	}
	return hex.EncodeToString(b[:])
}

var _ interface {
	Handle(context.Context, net.Conn)
} = (*SessionHandler)(nil)
