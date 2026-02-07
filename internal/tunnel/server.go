package tunnel

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"net"
	"sync"
	"sync/atomic"
	"time"
)

type ServerOptions struct {
	Enabled    bool
	ListenAddr string
	Transport  string
	AuthToken  string
	QUIC       QUICOptions
	Logger     *slog.Logger
	Manager    *Manager
}

type Server struct {
	opts ServerOptions

	tr Transport
	ln TransportListener

	wg        sync.WaitGroup
	listening atomic.Bool

	idSeq atomic.Uint64
}

func NewServer(opts ServerOptions) (*Server, error) {
	if opts.Logger == nil {
		opts.Logger = slog.Default()
	}
	if opts.Manager == nil {
		opts.Manager = NewManager(opts.Logger)
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
	return &Server{opts: opts, tr: tr}, nil
}

func (s *Server) Manager() *Manager { return s.opts.Manager }

func (s *Server) IsListening() bool { return s.listening.Load() }

func (s *Server) Addr() net.Addr {
	if s.ln == nil {
		return nil
	}
	return s.ln.Addr()
}

func (s *Server) ListenAndServe(ctx context.Context) error {
	if !s.opts.Enabled {
		return nil
	}
	if s.opts.ListenAddr == "" {
		return fmt.Errorf("tunnel: listen_addr is required when tunnel is enabled")
	}

	ln, err := s.tr.Listen(s.opts.ListenAddr, TransportListenOptions{QUIC: s.opts.QUIC})
	if err != nil {
		return err
	}
	s.ln = ln
	s.listening.Store(true)
	s.opts.Logger.Info("tunnel: listening", "addr", ln.Addr().String(), "transport", s.tr.Name())
	defer s.listening.Store(false)

	for {
		sess, err := ln.Accept(ctx)
		if err != nil {
			if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) {
				return err
			}
			// If the listener is closed, treat as graceful.
			if errors.Is(err, net.ErrClosed) {
				return nil
			}
			s.opts.Logger.Warn("tunnel: accept failed", "err", err)
			return err
		}

		s.wg.Add(1)
		go func(ts TransportSession) {
			defer s.wg.Done()
			s.handleSession(ctx, ts)
		}(sess)
	}
}

func (s *Server) handleSession(ctx context.Context, sess TransportSession) {
	clientID := fmt.Sprintf("c-%d", s.idSeq.Add(1))
	remote := ""
	if ra := sess.RemoteAddr(); ra != nil {
		remote = ra.String()
	}

	// First stream must be the register request.
	regStream, err := sess.AcceptStream(ctx)
	if err != nil {
		s.opts.Logger.Warn("tunnel: accept register stream failed", "client", remote, "err", err)
		_ = sess.Close()
		return
	}
	defer regStream.Close()

	req, err := readRegisterRequest(regStream)
	if err != nil {
		s.opts.Logger.Warn("tunnel: read register failed", "client", remote, "err", err)
		_ = sess.Close()
		return
	}
	if s.opts.AuthToken != "" && req.Token != s.opts.AuthToken {
		s.opts.Logger.Warn("tunnel: bad token", "client", remote)
		_ = sess.Close()
		return
	}

	if err := s.opts.Manager.RegisterClient(clientID, sess, req.Services); err != nil {
		s.opts.Logger.Warn("tunnel: register client failed", "client", remote, "err", err)
		_ = sess.Close()
		return
	}

	// Keep a lightweight accept loop running so we can notice disconnects and
	// also avoid backpressure if the client ever opens a stream unexpectedly.
	s.opts.Logger.Info("tunnel: client connected", "cid", clientID, "client", remote, "services", len(req.Services))
	defer func() {
		s.opts.Manager.UnregisterClient(clientID)
		s.opts.Logger.Info("tunnel: client disconnected", "cid", clientID, "client", remote)
	}()

	for {
		st, err := sess.AcceptStream(ctx)
		if err != nil {
			return
		}
		// Unexpected stream opened by client: close quietly.
		_ = st.SetDeadline(time.Now().Add(1 * time.Second))
		_ = st.Close()
	}
}

func (s *Server) Shutdown(ctx context.Context) error {
	if s.ln != nil {
		_ = s.ln.Close()
	}

	done := make(chan struct{})
	go func() {
		s.wg.Wait()
		close(done)
	}()

	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-done:
		return nil
	}
}
