package proxy

import (
	"bytes"
	"context"
	"errors"
	"io"
	"log/slog"
	"net"
	"strconv"
	"strings"
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

	// DefaultUpstreamPort is used when a resolved upstream address does not
	// include a port (e.g. a bare hostname like "backend.example.com").
	//
	// If the incoming connection is a Minecraft handshake and the port can be
	// parsed safely from the captured prelude, that port is preferred.
	DefaultUpstreamPort int
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
	upstreamAddr = normalizeUpstreamAddr(upstreamAddr, host, captured, opts.DefaultUpstreamPort)
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

func normalizeUpstreamAddr(upstreamAddr string, routedHost string, capturedPrelude []byte, defaultPort int) string {
	addr := strings.TrimSpace(upstreamAddr)
	if addr == "" {
		return addr
	}
	if defaultPort <= 0 {
		defaultPort = 25565
	}
	if !upstreamNeedsPort(addr) {
		return addr
	}

	// Prefer the port from a Minecraft handshake if present and it matches the
	// routed hostname. This avoids accidentally interpreting non-Minecraft
	// traffic as a handshake.
	port := defaultPort
	if h, p, ok := protocol.TryParseMinecraftHandshakeHostPort(capturedPrelude, 256*1024, 255); ok {
		routedHost = strings.TrimSpace(strings.ToLower(routedHost))
		if routedHost != "" && h == routedHost {
			port = int(p)
		}
	}

	host := stripOptionalIPv6Brackets(addr)
	return net.JoinHostPort(host, strconv.Itoa(port))
}

func upstreamNeedsPort(addr string) bool {
	addr = strings.TrimSpace(addr)
	if addr == "" {
		return false
	}

	// Bracketed IPv6.
	if strings.HasPrefix(addr, "[") {
		// If it contains ]: it's already host:port.
		if strings.Contains(addr, "]:") {
			return false
		}
		// If it ends with ], treat as a bare host.
		return strings.HasSuffix(addr, "]")
	}

	colons := strings.Count(addr, ":")
	if colons == 0 {
		return true
	}
	if colons > 1 {
		// Likely an unbracketed IPv6 literal without port.
		return true
	}

	// Single colon: attempt host:port parse.
	_, _, err := net.SplitHostPort(addr)
	if err == nil {
		return false
	}
	// Only treat it as missing-port when that's what SplitHostPort indicates.
	// For invalid ports like "host:abc", let the dial fail rather than
	// silently overriding the address.
	return strings.Contains(err.Error(), "missing port in address")
}

func stripOptionalIPv6Brackets(host string) string {
	host = strings.TrimSpace(host)
	if strings.HasPrefix(host, "[") && strings.HasSuffix(host, "]") {
		return strings.TrimSuffix(strings.TrimPrefix(host, "["), "]")
	}
	return host
}

var sidSeq atomic.Uint64

func newSessionID() string {
	return strconv.FormatUint(sidSeq.Add(1), 10)
}

var _ interface {
	Handle(context.Context, net.Conn)
} = (*SessionHandler)(nil)
