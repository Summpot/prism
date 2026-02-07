package tunnel

import (
	"context"
	"crypto/tls"
	"errors"
	"net"
	"time"

	"github.com/quic-go/quic-go"
)

type quicTransport struct{}

func NewQUICTransport() Transport { return quicTransport{} }

func (quicTransport) Name() string { return "quic" }

func (quicTransport) Listen(addr string, opts TransportListenOptions) (TransportListener, error) {
	cert, generated, err := loadOrGenerateServerCertificate(opts.QUIC.CertFile, opts.QUIC.KeyFile)
	if err != nil {
		return nil, err
	}

	tlsConf := &tls.Config{
		Certificates: []tls.Certificate{cert},
		NextProtos:   defaultALPN(opts.QUIC.NextProtos),
	}
	// generated is returned to allow logging at the call site (server), but we
	// don't plumb it through the interface for now.
	_ = generated

	ln, err := quic.ListenAddr(addr, tlsConf, &quic.Config{
		MaxIdleTimeout:  60 * time.Second,
		KeepAlivePeriod: 20 * time.Second,
	})
	if err != nil {
		return nil, err
	}
	return &quicListener{ln: ln}, nil
}

func (quicTransport) Dial(ctx context.Context, addr string, opts TransportDialOptions) (TransportSession, error) {
	tlsConf := &tls.Config{
		InsecureSkipVerify: opts.QUIC.InsecureSkipVerify,
		ServerName:         opts.QUIC.ServerName,
		NextProtos:         defaultALPN(opts.QUIC.NextProtos),
	}
	c, err := quic.DialAddr(ctx, addr, tlsConf, &quic.Config{
		MaxIdleTimeout:  60 * time.Second,
		KeepAlivePeriod: 20 * time.Second,
	})
	if err != nil {
		return nil, err
	}
	return &quicSession{c: c}, nil
}

type quicListener struct {
	ln *quic.Listener
}

func (l *quicListener) Accept(ctx context.Context) (TransportSession, error) {
	c, err := l.ln.Accept(ctx)
	if err != nil {
		return nil, err
	}
	return &quicSession{c: c}, nil
}

func (l *quicListener) Close() error   { return l.ln.Close() }
func (l *quicListener) Addr() net.Addr { return l.ln.Addr() }

type quicSession struct {
	c *quic.Conn
}

func (s *quicSession) OpenStream(ctx context.Context) (net.Conn, error) {
	st, err := s.c.OpenStreamSync(ctx)
	if err != nil {
		return nil, err
	}
	return &quicStreamConn{st: st, local: s.c.LocalAddr(), remote: s.c.RemoteAddr()}, nil
}

func (s *quicSession) AcceptStream(ctx context.Context) (net.Conn, error) {
	st, err := s.c.AcceptStream(ctx)
	if err != nil {
		return nil, err
	}
	return &quicStreamConn{st: st, local: s.c.LocalAddr(), remote: s.c.RemoteAddr()}, nil
}

func (s *quicSession) Close() error {
	// CloseWithError is recommended to unblock stream operations.
	err := s.c.CloseWithError(0, "")
	if errors.Is(err, net.ErrClosed) {
		return nil
	}
	return err
}

func (s *quicSession) RemoteAddr() net.Addr { return s.c.RemoteAddr() }
func (s *quicSession) LocalAddr() net.Addr  { return s.c.LocalAddr() }

type quicStreamConn struct {
	st     *quic.Stream
	local  net.Addr
	remote net.Addr
}

func (c *quicStreamConn) Read(p []byte) (int, error)  { return c.st.Read(p) }
func (c *quicStreamConn) Write(p []byte) (int, error) { return c.st.Write(p) }
func (c *quicStreamConn) Close() error                { return c.st.Close() }
func (c *quicStreamConn) LocalAddr() net.Addr         { return c.local }
func (c *quicStreamConn) RemoteAddr() net.Addr        { return c.remote }
func (c *quicStreamConn) SetDeadline(t time.Time) error {
	return c.st.SetDeadline(t)
}
func (c *quicStreamConn) SetReadDeadline(t time.Time) error {
	return c.st.SetReadDeadline(t)
}
func (c *quicStreamConn) SetWriteDeadline(t time.Time) error {
	return c.st.SetWriteDeadline(t)
}

var _ net.Conn = (*quicStreamConn)(nil)
