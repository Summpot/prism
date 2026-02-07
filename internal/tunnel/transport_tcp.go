package tunnel

import (
	"context"
	"errors"
	"net"

	"github.com/hashicorp/yamux"
)

type tcpTransport struct{}

func NewTCPTransport() Transport { return tcpTransport{} }

func (tcpTransport) Name() string { return "tcp" }

func (tcpTransport) Listen(addr string, _ TransportListenOptions) (TransportListener, error) {
	ln, err := net.Listen("tcp", addr)
	if err != nil {
		return nil, err
	}
	return &tcpListener{ln: ln}, nil
}

func (tcpTransport) Dial(ctx context.Context, addr string, _ TransportDialOptions) (TransportSession, error) {
	var d net.Dialer
	c, err := d.DialContext(ctx, "tcp", addr)
	if err != nil {
		return nil, err
	}
	sess, err := yamux.Client(c, nil)
	if err != nil {
		_ = c.Close()
		return nil, err
	}
	return &yamuxSession{sess: sess, raw: c}, nil
}

type tcpListener struct {
	ln net.Listener
}

func (l *tcpListener) Accept(ctx context.Context) (TransportSession, error) {
	type res struct {
		c   net.Conn
		err error
	}
	ch := make(chan res, 1)
	go func() {
		c, err := l.ln.Accept()
		ch <- res{c: c, err: err}
	}()
	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	case r := <-ch:
		if r.err != nil {
			return nil, r.err
		}
		sess, err := yamux.Server(r.c, nil)
		if err != nil {
			_ = r.c.Close()
			return nil, err
		}
		return &yamuxSession{sess: sess, raw: r.c}, nil
	}
}

func (l *tcpListener) Close() error   { return l.ln.Close() }
func (l *tcpListener) Addr() net.Addr { return l.ln.Addr() }

type yamuxSession struct {
	sess *yamux.Session
	raw  net.Conn
}

func (s *yamuxSession) OpenStream(ctx context.Context) (net.Conn, error) {
	type res struct {
		st  *yamux.Stream
		err error
	}
	ch := make(chan res, 1)
	go func() {
		st, err := s.sess.OpenStream()
		ch <- res{st: st, err: err}
	}()
	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	case r := <-ch:
		return r.st, r.err
	}
}

func (s *yamuxSession) AcceptStream(ctx context.Context) (net.Conn, error) {
	type res struct {
		st  *yamux.Stream
		err error
	}
	ch := make(chan res, 1)
	go func() {
		st, err := s.sess.AcceptStream()
		ch <- res{st: st, err: err}
	}()
	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	case r := <-ch:
		return r.st, r.err
	}
}

func (s *yamuxSession) Close() error {
	// Close session first to unblock Open/Accept.
	err := s.sess.Close()
	// Ensure underlying conn is closed too.
	if s.raw != nil {
		err2 := s.raw.Close()
		if err == nil {
			err = err2
		}
	}
	if errors.Is(err, net.ErrClosed) {
		return nil
	}
	return err
}

func (s *yamuxSession) RemoteAddr() net.Addr { return s.raw.RemoteAddr() }
func (s *yamuxSession) LocalAddr() net.Addr  { return s.raw.LocalAddr() }
