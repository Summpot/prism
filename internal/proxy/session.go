package proxy

import (
	"bytes"
	"context"
	"errors"
	"io"
	"log/slog"
	"net"
	"strconv"
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
	Logger   *slog.Logger
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
	logger := opts.Logger
	if logger == nil {
		logger = slog.Default()
	}

	sid := newSessionID()
	clientAddr := ""
	if conn != nil && conn.RemoteAddr() != nil {
		clientAddr = conn.RemoteAddr().String()
	}
	start := time.Now()
	if logger.Enabled(ctx, slog.LevelDebug) {
		logger.Debug("proxy: session started", "sid", sid, "client", clientAddr)
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
			if logger.Enabled(ctx, slog.LevelDebug) {
				logger.Debug("proxy: handshake read failed", "sid", sid, "client", clientAddr, "captured_bytes", len(captured), "err", err)
			}
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
		if errors.Is(perr, protocol.ErrNoMatch) {
			if logger.Enabled(ctx, slog.LevelDebug) {
				logger.Debug("proxy: no routing header match", "sid", sid, "client", clientAddr, "parser", opts.Parser.Name(), "captured_bytes", len(captured))
			}
			return
		}
		// Fatal parse error: drop the connection (can't route).
		logger.Warn("proxy: routing header parse failed", "sid", sid, "client", clientAddr, "parser", opts.Parser.Name(), "captured_bytes", len(captured), "err", perr)
		return
	}
	if host == "" {
		if logger.Enabled(ctx, slog.LevelDebug) {
			logger.Debug("proxy: exceeded max header bytes without host", "sid", sid, "client", clientAddr, "max_header_bytes", maxHeader)
		}
		return
	}

	// Clear handshake read deadline; the idle timeout (if set) applies from here.
	_ = conn.SetReadDeadline(time.Time{})

	upstreamAddr, ok := opts.Resolver.Resolve(host)
	if !ok {
		if logger.Enabled(ctx, slog.LevelDebug) {
			logger.Debug("proxy: no route for host", "sid", sid, "client", clientAddr, "host", host)
		}
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
		logger.Warn("proxy: upstream dial failed", "sid", sid, "client", clientAddr, "host", host, "upstream", upstreamAddr, "err", err)
		return
	}

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
	if logger.Enabled(ctx, slog.LevelDebug) {
		logger.Debug("proxy: routed", "sid", sid, "client", clientAddr, "host", host, "upstream", upstreamAddr)
	}

	initial := io.MultiReader(bytes.NewReader(captured), conn)
	err = opts.Bridge.Proxy(ctx, conn, up, initial)
	dur := time.Since(start)
	if err != nil {
		if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) {
			if logger.Enabled(ctx, slog.LevelDebug) {
				logger.Debug("proxy: session ended", "sid", sid, "client", clientAddr, "host", host, "upstream", upstreamAddr, "duration_ms", dur.Milliseconds(), "err", err)
			}
			return
		}
		logger.Warn("proxy: session ended with error", "sid", sid, "client", clientAddr, "host", host, "upstream", upstreamAddr, "duration_ms", dur.Milliseconds(), "err", err)
		return
	}
	if logger.Enabled(ctx, slog.LevelDebug) {
		logger.Debug("proxy: session ended", "sid", sid, "client", clientAddr, "host", host, "upstream", upstreamAddr, "duration_ms", dur.Milliseconds())
	}
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}

var sidSeq atomic.Uint64

func newSessionID() string {
	return strconv.FormatUint(sidSeq.Add(1), 10)
}

var _ interface {
	Handle(context.Context, net.Conn)
} = (*SessionHandler)(nil)
