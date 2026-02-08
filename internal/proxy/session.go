package proxy

import (
	"bytes"
	"context"
	"encoding/binary"
	"errors"
	"fmt"
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
	"prism/pkg/mcproto"
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

	// StatusCache enables caching of Minecraft Status (server list ping) responses.
	// If nil, a package-level default cache is used.
	StatusCache *StatusCache

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

	res, ok := opts.Resolver.Resolve(host)
	if !ok {
		if logger.Enabled(ctx, slog.LevelDebug) {
			logger.Debug("proxy: no route for host", "sid", sid, "client", clientAddr, "host", host)
		}
		return
	}
	if opts.Metrics != nil {
		opts.Metrics.AddRouteHit(host)
	}

	// Minecraft Status (server list ping) caching.
	// This is a fast path that can avoid dialing an upstream for repeated pings.
	if res.CachePingTTL > 0 {
		if md, handshakeLen, ok := protocol.TryParseMinecraftHandshakeMetadata(captured, 256*1024, 255); ok && md.NextState == 1 {
			cache := opts.StatusCache
			if cache == nil {
				cache = DefaultStatusCache()
			}

			// Read the status request packet from the client stream (it may already be in captured bytes).
			clientR := io.MultiReader(bytes.NewReader(captured[handshakeLen:]), conn)
			statusReqRaw, pid, err := readPacketRaw(clientR, 64*1024)
			if err != nil {
				return
			}
			if pid == 0 {
				// Try upstreams in resolver-specified order.
				protoVer := md.ProtocolVersion
				for _, u := range res.Upstreams {
					up := u
					if !isTunnelUpstream(strings.TrimSpace(up)) {
						up = normalizeUpstreamAddr(up, host, captured[:handshakeLen], opts.DefaultUpstreamPort)
					}
					key := StatusCacheKey{Upstream: up, ProtocolVersion: protoVer}
					cached, ok := cache.Get(key)
					if ok {
						// Return cached status response and reply to ping.
						if _, werr := conn.Write(cached); werr != nil {
							return
						}
						_ = replyPingPong(conn, clientR)
						return
					}

					resp, lerr := cache.GetOrLoad(ctx, key, res.CachePingTTL, func(ctx context.Context) ([]byte, error) {
						// Dial upstream and fetch one status response.
						dst, err := opts.Dialer.DialContext(ctx, "tcp", up)
						if err != nil {
							return nil, err
						}
						defer dst.Close()

						// Write handshake + status request upstream.
						if _, err := dst.Write(captured[:handshakeLen]); err != nil {
							return nil, err
						}
						if _, err := dst.Write(statusReqRaw); err != nil {
							return nil, err
						}

						// Read exactly one packet (the status response) and cache its raw bytes.
						respRaw, respID, err := readPacketRaw(dst, 512*1024)
						if err != nil {
							return nil, err
						}
						if respID != 0 {
							return nil, fmt.Errorf("protocol: unexpected status response packet id %d", respID)
						}
						return respRaw, nil
					})
					if lerr != nil {
						// Try next upstream.
						continue
					}
					if _, werr := conn.Write(resp); werr != nil {
						return
					}
					_ = replyPingPong(conn, clientR)
					return
				}
				// All upstreams failed; drop.
				return
			}
			// Not a status request packet; drop.
			return
		}
	}

	if opts.Timeouts.IdleTimeout > 0 {
		_ = conn.SetDeadline(time.Now().Add(opts.Timeouts.IdleTimeout))
	}

	// Dial upstream(s) with failover.
	var (
		up           net.Conn
		upstreamAddr string
		lastErr      error
	)
	for _, cand := range res.Upstreams {
		addr := cand
		if !isTunnelUpstream(strings.TrimSpace(addr)) {
			addr = normalizeUpstreamAddr(addr, host, captured, opts.DefaultUpstreamPort)
		}
		c, err := opts.Dialer.DialContext(ctx, "tcp", addr)
		if err != nil {
			lastErr = err
			continue
		}
		up = c
		upstreamAddr = addr
		break
	}
	if up == nil {
		logger.Warn("proxy: upstream dial failed", "sid", sid, "client", clientAddr, "host", host, "upstream_candidates", len(res.Upstreams), "err", lastErr)
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
	err := opts.Bridge.Proxy(ctx, conn, up, initial)
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

// replyPingPong reads an optional Status Ping packet (id=1) from src and replies with a Pong.
// If the client closes without sending a ping, this returns nil.
func replyPingPong(dst net.Conn, src io.Reader) error {
	// Try to read the next packet. If EOF, that's fine.
	raw, pid, err := readPacketRaw(src, 64*1024)
	if err != nil {
		if errors.Is(err, io.EOF) {
			return nil
		}
		return err
	}
	if pid != 1 {
		// Not a ping packet; ignore.
		_ = raw
		return nil
	}
	// Decode payload long.
	// raw = [len VarInt][packet bytes]. Re-parse packet bytes after length.
	packet, err := stripLengthPrefix(raw)
	if err != nil {
		return err
	}
	br := bytes.NewReader(packet)
	if _, _, err := mcproto.ReadVarInt(br); err != nil {
		return err
	}
	var buf [8]byte
	if _, err := io.ReadFull(br, buf[:]); err != nil {
		return err
	}
	val := int64(binary.BigEndian.Uint64(buf[:]))

	pong := buildPongPacket(val)
	_, err = dst.Write(pong)
	return err
}

func buildPongPacket(v int64) []byte {
	var payload bytes.Buffer
	_, _ = mcproto.WriteVarInt(&payload, 1) // packet id
	var b [8]byte
	binary.BigEndian.PutUint64(b[:], uint64(v))
	_, _ = payload.Write(b[:])

	var out bytes.Buffer
	_, _ = mcproto.WriteVarInt(&out, int32(payload.Len()))
	_, _ = out.Write(payload.Bytes())
	return out.Bytes()
}

func stripLengthPrefix(raw []byte) ([]byte, error) {
	br := bytes.NewReader(raw)
	ln, n, err := mcproto.ReadVarInt(br)
	if err != nil {
		return nil, err
	}
	if ln < 0 {
		return nil, fmt.Errorf("protocol: negative packet length")
	}
	if int(ln) != len(raw)-n {
		// raw might include extra bytes, but in our use it shouldn't.
		if int(ln) > len(raw)-n {
			return nil, fmt.Errorf("protocol: truncated packet")
		}
	}
	return raw[n:], nil
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
	if isTunnelUpstream(addr) {
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
	if isTunnelUpstream(addr) {
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

func isTunnelUpstream(addr string) bool {
	addr = strings.TrimSpace(strings.ToLower(addr))
	return strings.HasPrefix(addr, "tunnel:") || strings.HasPrefix(addr, "tunnel://")
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
