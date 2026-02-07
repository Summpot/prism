package tunnel

import (
	"context"
	"net"

	"github.com/hashicorp/yamux"
	"github.com/xtaci/kcp-go/v5"
)

// In Prism config we expose this as transport "udp" to match the user's wording.
// Under the hood this is KCP (reliable UDP) so it can carry stream traffic.

type udpTransport struct{}

func NewUDPTransport() Transport { return udpTransport{} }

func (udpTransport) Name() string { return "udp" }

func (udpTransport) Listen(addr string, _ TransportListenOptions) (TransportListener, error) {
	ln, err := kcp.ListenWithOptions(addr, nil, 10, 3)
	if err != nil {
		return nil, err
	}
	return &kcpListener{ln: ln}, nil
}

func (udpTransport) Dial(ctx context.Context, addr string, _ TransportDialOptions) (TransportSession, error) {
	// kcp-go does not accept a context; emulate with a goroutine.
	type res struct {
		sess *kcp.UDPSession
		err  error
	}
	ch := make(chan res, 1)
	go func() {
		c, err := kcp.DialWithOptions(addr, nil, 10, 3)
		if err == nil {
			c.SetNoDelay(1, 20, 2, 1)
			c.SetWindowSize(1024, 1024)
		}
		ch <- res{sess: c, err: err}
	}()
	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	case r := <-ch:
		if r.err != nil {
			return nil, r.err
		}
		ys, err := yamux.Client(r.sess, nil)
		if err != nil {
			_ = r.sess.Close()
			return nil, err
		}
		return &yamuxSession{sess: ys, raw: r.sess}, nil
	}
}

type kcpListener struct {
	ln *kcp.Listener
}

func (l *kcpListener) Accept(ctx context.Context) (TransportSession, error) {
	// Listener.AcceptKCP doesn't take a context.
	type res struct {
		c   *kcp.UDPSession
		err error
	}
	ch := make(chan res, 1)
	go func() {
		c, err := l.ln.AcceptKCP()
		ch <- res{c: c, err: err}
	}()
	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	case r := <-ch:
		if r.err != nil {
			return nil, r.err
		}
		r.c.SetNoDelay(1, 20, 2, 1)
		r.c.SetWindowSize(1024, 1024)
		ys, err := yamux.Server(r.c, nil)
		if err != nil {
			_ = r.c.Close()
			return nil, err
		}
		return &yamuxSession{sess: ys, raw: r.c}, nil
	}
}

func (l *kcpListener) Close() error   { return l.ln.Close() }
func (l *kcpListener) Addr() net.Addr { return l.ln.Addr() }
