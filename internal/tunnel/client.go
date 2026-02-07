package tunnel

import (
	"context"
	"errors"
	"log/slog"
	"net"
	"strings"
	"sync"
	"time"
)

type ClientOptions struct {
	ServerAddr string
	Transport  string
	AuthToken  string
	Services   []RegisteredService

	QUIC QUICDialOptions

	Logger      *slog.Logger
	BufSize     int
	DialTimeout time.Duration
}

type Client struct {
	opts   ClientOptions
	tr     Transport
	bridge *Bridge

	mu       sync.RWMutex
	localMap map[string]RegisteredService // service -> metadata (proto/local)
}

func NewClient(opts ClientOptions) (*Client, error) {
	if opts.Logger == nil {
		opts.Logger = slog.Default()
	}
	name, err := ParseTransport(opts.Transport)
	if err != nil {
		return nil, err
	}
	opts.Transport = name
	tr, err := TransportByName(name)
	if err != nil {
		return nil, err
	}
	c := &Client{opts: opts, tr: tr, bridge: NewBridge(opts.BufSize), localMap: map[string]RegisteredService{}}
	for _, s := range opts.Services {
		name := strings.TrimSpace(s.Name)
		addr := strings.TrimSpace(s.LocalAddr)
		if name == "" || addr == "" {
			continue
		}
		proto := strings.TrimSpace(strings.ToLower(s.Proto))
		if proto == "" {
			proto = "tcp"
		}
		c.localMap[name] = RegisteredService{Name: name, Proto: proto, LocalAddr: addr, RemoteAddr: strings.TrimSpace(s.RemoteAddr)}
	}
	if opts.DialTimeout <= 0 {
		c.opts.DialTimeout = 5 * time.Second
	}
	return c, nil
}

func (c *Client) Run(ctx context.Context) error {
	if strings.TrimSpace(c.opts.ServerAddr) == "" {
		return errors.New("tunnel: client server_addr is required")
	}

	backoff := 1 * time.Second
	for {
		if err := ctx.Err(); err != nil {
			return err
		}
		err := c.runOnce(ctx)
		if err == nil {
			return nil
		}
		if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) {
			return err
		}
		c.opts.Logger.Warn("tunnel: disconnected; retrying", "transport", c.tr.Name(), "server", c.opts.ServerAddr, "err", err, "backoff", backoff.String())
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(backoff):
		}
		if backoff < 10*time.Second {
			backoff *= 2
			if backoff > 10*time.Second {
				backoff = 10 * time.Second
			}
		}
	}
}

func (c *Client) runOnce(ctx context.Context) error {
	dialCtx, cancel := context.WithTimeout(ctx, c.opts.DialTimeout)
	defer cancel()

	sess, err := c.tr.Dial(dialCtx, c.opts.ServerAddr, TransportDialOptions{QUIC: c.opts.QUIC})
	if err != nil {
		return err
	}
	defer sess.Close()

	// Register.
	st, err := sess.OpenStream(ctx)
	if err != nil {
		return err
	}
	regReq := RegisterRequest{Token: c.opts.AuthToken, Services: c.opts.Services}
	if err := writeRegisterRequest(st, regReq); err != nil {
		_ = st.Close()
		return err
	}
	_ = st.Close()
	c.opts.Logger.Info("tunnel: connected", "transport", c.tr.Name(), "server", c.opts.ServerAddr, "services", len(c.opts.Services))

	// Serve incoming proxy streams.
	for {
		st, err := sess.AcceptStream(ctx)
		if err != nil {
			return err
		}
		go c.handleStream(ctx, st)
	}
}

func (c *Client) handleStream(ctx context.Context, st net.Conn) {
	defer st.Close()

	kind, svc, err := readProxyStreamHeader(st)
	if err != nil {
		// Server may have opened an unknown stream type.
		c.opts.Logger.Debug("tunnel: stream header error", "err", err)
		return
	}

	c.mu.RLock()
	meta := c.localMap[svc]
	c.mu.RUnlock()
	local := strings.TrimSpace(meta.LocalAddr)
	if local == "" {
		c.opts.Logger.Warn("tunnel: unknown service", "service", svc)
		return
	}

	switch kind {
	case ProxyStreamTCP:
		var d net.Dialer
		up, err := d.DialContext(ctx, "tcp", local)
		if err != nil {
			c.opts.Logger.Warn("tunnel: dial local failed", "service", svc, "local", local, "err", err)
			return
		}
		_ = c.bridge.Proxy(ctx, st, up)
	case ProxyStreamUDP:
		var d net.Dialer
		up, err := d.DialContext(ctx, "udp", local)
		if err != nil {
			c.opts.Logger.Warn("tunnel: dial local failed", "service", svc, "local", local, "err", err)
			return
		}
		// After the proxy header, the stream carries framed UDP datagrams.
		dg := NewDatagramConn(st)
		_ = c.bridge.Proxy(ctx, dg, up)
	default:
		c.opts.Logger.Debug("tunnel: unknown proxy stream kind", "kind", string(kind), "service", svc)
		return
	}
}
