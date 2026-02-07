package proxy

import (
	"context"
	"errors"
	"log/slog"
	"net"
	"sync"
	"sync/atomic"
	"time"

	"prism/internal/server"
)

type UDPForwarderOptions struct {
	Upstream string
	Dialer   Dialer

	// IdleTimeout controls how long an inactive client session is kept.
	// If zero, sessions are kept until shutdown.
	IdleTimeout time.Duration

	Logger *slog.Logger
}

// UDPForwarder implements a simple UDP NAT-style forwarder.
//
// For each unique client source address, it dials the configured upstream and
// forwards packets in both directions until idle timeout or shutdown.
//
// This enables UDP-based games/protocols where hostname-based routing is not
// applicable.
//
//nolint:revive // name is clear enough.
type UDPForwarder struct {
	v atomic.Value // UDPForwarderOptions

	mu       sync.Mutex
	sessions map[string]*udpSession
	once     sync.Once
}

type udpSession struct {
	up       net.Conn
	src      net.Addr
	lastSeen time.Time

	cancel context.CancelFunc
}

func NewUDPForwarder(opts UDPForwarderOptions) *UDPForwarder {
	if opts.Logger == nil {
		opts.Logger = slog.Default()
	}
	f := &UDPForwarder{sessions: map[string]*udpSession{}}
	f.v.Store(opts)
	return f
}

func (f *UDPForwarder) Update(opts UDPForwarderOptions) {
	if opts.Logger == nil {
		opts.Logger = slog.Default()
	}
	f.v.Store(opts)
}

func (f *UDPForwarder) HandlePacket(ctx context.Context, pc net.PacketConn, src net.Addr, payload []byte) {
	optsAny := f.v.Load()
	opts, _ := optsAny.(UDPForwarderOptions)
	if opts.Dialer == nil || pc == nil || src == nil {
		return
	}
	if opts.Upstream == "" {
		return
	}

	// Start sweeper lazily.
	f.once.Do(func() {
		go f.sweepLoop(ctx)
	})

	key := src.String()
	var s *udpSession

	f.mu.Lock()
	s = f.sessions[key]
	if s == nil {
		sessCtx, cancel := context.WithCancel(ctx)
		up, err := opts.Dialer.DialContext(sessCtx, "udp", opts.Upstream)
		if err != nil {
			cancel()
			if opts.Logger.Enabled(ctx, slog.LevelDebug) {
				opts.Logger.Debug("proxy: udp dial upstream failed", "client", key, "upstream", opts.Upstream, "err", err)
			}
			f.mu.Unlock()
			return
		}
		s = &udpSession{up: up, src: src, lastSeen: time.Now(), cancel: cancel}
		f.sessions[key] = s
		go f.upstreamReadLoop(sessCtx, pc, key, s)
	}
	s.lastSeen = time.Now()
	f.mu.Unlock()

	// Forward client -> upstream.
	_, err := s.up.Write(payload)
	if err != nil && !errors.Is(err, net.ErrClosed) {
		if opts.Logger.Enabled(ctx, slog.LevelDebug) {
			opts.Logger.Debug("proxy: udp write upstream failed", "client", key, "upstream", opts.Upstream, "err", err)
		}
		f.closeSession(key)
		return
	}
}

func (f *UDPForwarder) upstreamReadLoop(ctx context.Context, pc net.PacketConn, key string, s *udpSession) {
	defer f.closeSession(key)

	buf := make([]byte, 64*1024)
	for {
		if err := ctx.Err(); err != nil {
			return
		}
		// Use a short deadline to ensure ctx cancellation is observed.
		_ = s.up.SetReadDeadline(time.Now().Add(1 * time.Second))
		n, err := s.up.Read(buf)
		if err != nil {
			if ne, ok := err.(net.Error); ok && ne.Timeout() {
				continue
			}
			if errors.Is(err, net.ErrClosed) {
				return
			}
			return
		}
		if n <= 0 {
			continue
		}
		_, _ = pc.WriteTo(buf[:n], s.src)
	}
}

func (f *UDPForwarder) sweepLoop(ctx context.Context) {
	tick := time.NewTicker(1 * time.Second)
	defer tick.Stop()

	for {
		select {
		case <-ctx.Done():
			f.closeAll()
			return
		case <-tick.C:
			optsAny := f.v.Load()
			opts, _ := optsAny.(UDPForwarderOptions)
			if opts.IdleTimeout <= 0 {
				continue
			}
			now := time.Now()
			var toClose []string

			f.mu.Lock()
			for k, s := range f.sessions {
				if s == nil {
					continue
				}
				if now.Sub(s.lastSeen) > opts.IdleTimeout {
					toClose = append(toClose, k)
				}
			}
			f.mu.Unlock()

			for _, k := range toClose {
				f.closeSession(k)
			}
		}
	}
}

func (f *UDPForwarder) closeSession(key string) {
	f.mu.Lock()
	s := f.sessions[key]
	if s == nil {
		f.mu.Unlock()
		return
	}
	delete(f.sessions, key)
	f.mu.Unlock()

	if s.cancel != nil {
		s.cancel()
	}
	if s.up != nil {
		_ = s.up.Close()
	}
}

func (f *UDPForwarder) closeAll() {
	f.mu.Lock()
	keys := make([]string, 0, len(f.sessions))
	for k := range f.sessions {
		keys = append(keys, k)
	}
	f.mu.Unlock()

	for _, k := range keys {
		f.closeSession(k)
	}
}

var _ server.PacketHandler = (*UDPForwarder)(nil)
