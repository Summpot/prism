package server

import (
	"context"
	"errors"
	"log/slog"
	"net"
	"sync"
	"sync/atomic"
)

type ConnectionHandler interface {
	Handle(ctx context.Context, conn net.Conn)
}

type TCPServer struct {
	addr    string
	h       ConnectionHandler
	logger  *slog.Logger
	metrics interface {
		IncActive()
		DecActive()
	}

	ln        net.Listener
	listening atomic.Bool

	wg sync.WaitGroup
}

func NewTCPServer(addr string, h ConnectionHandler, metrics any, logger *slog.Logger) *TCPServer {
	var m interface {
		IncActive()
		DecActive()
	}
	if metrics != nil {
		m, _ = metrics.(interface {
			IncActive()
			DecActive()
		})
	}
	if logger == nil {
		logger = slog.Default()
	}
	return &TCPServer{addr: addr, h: h, metrics: m, logger: logger}
}

func (s *TCPServer) IsListening() bool {
	return s.listening.Load()
}

func (s *TCPServer) ListenAndServe(ctx context.Context) error {
	ln, err := net.Listen("tcp", s.addr)
	if err != nil {
		if s.logger != nil {
			s.logger.Error("server: listen failed", "addr", s.addr, "err", err)
		}
		return err
	}
	s.ln = ln
	s.listening.Store(true)
	if s.logger != nil {
		s.logger.Info("server: listening", "addr", s.addr)
	}
	defer s.listening.Store(false)

	for {
		conn, err := ln.Accept()
		if err != nil {
			if errors.Is(err, net.ErrClosed) {
				if s.logger != nil {
					s.logger.Info("server: listener closed")
				}
				return nil
			}
			if s.logger != nil {
				s.logger.Error("server: accept failed", "err", err)
			}
			return err
		}

		s.wg.Add(1)
		go func(c net.Conn) {
			defer s.wg.Done()
			s.h.Handle(ctx, c)
		}(conn)
	}
}

func (s *TCPServer) Shutdown(ctx context.Context) error {
	if s.logger != nil {
		s.logger.Info("server: shutdown requested")
	}
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
		if s.logger != nil {
			s.logger.Warn("server: shutdown timed out", "err", ctx.Err())
		}
		return ctx.Err()
	case <-done:
		if s.logger != nil {
			s.logger.Info("server: shutdown complete")
		}
		return nil
	}
}
