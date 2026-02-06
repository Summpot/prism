package server

import (
	"context"
	"errors"
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
	metrics interface {
		IncActive()
		DecActive()
	}

	ln        net.Listener
	listening atomic.Bool

	wg sync.WaitGroup
}

func NewTCPServer(addr string, h ConnectionHandler, metrics any) *TCPServer {
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
	return &TCPServer{addr: addr, h: h, metrics: m}
}

func (s *TCPServer) IsListening() bool {
	return s.listening.Load()
}

func (s *TCPServer) ListenAndServe(ctx context.Context) error {
	ln, err := net.Listen("tcp", s.addr)
	if err != nil {
		return err
	}
	s.ln = ln
	s.listening.Store(true)
	defer s.listening.Store(false)

	for {
		conn, err := ln.Accept()
		if err != nil {
			if errors.Is(err, net.ErrClosed) {
				return nil
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
