package server

import (
	"context"
	"errors"
	"log/slog"
	"net"
	"sync/atomic"
)

// PacketHandler processes a single UDP datagram.
//
// The payload slice is only valid for the duration of the call; handlers must
// copy it if they retain it or use it asynchronously.
//
// Handlers may write responses back to src via pc.WriteTo.
//
// Implementations should be fast and avoid blocking for long periods to keep
// the receive loop responsive.
//
// Cancellation should flow from ctx.
//
//nolint:revive // stuttering is fine here (UDPServer).
type PacketHandler interface {
	HandlePacket(ctx context.Context, pc net.PacketConn, src net.Addr, payload []byte)
}

type UDPServer struct {
	addr      string
	h         PacketHandler
	logger    *slog.Logger
	pc        net.PacketConn
	listening atomic.Bool
}

func NewUDPServer(addr string, h PacketHandler, logger *slog.Logger) *UDPServer {
	if logger == nil {
		logger = slog.Default()
	}
	return &UDPServer{addr: addr, h: h, logger: logger}
}

func (s *UDPServer) IsListening() bool {
	return s.listening.Load()
}

func (s *UDPServer) Addr() net.Addr {
	if s.pc == nil {
		return nil
	}
	return s.pc.LocalAddr()
}

func (s *UDPServer) ListenAndServe(ctx context.Context) error {
	pc, err := net.ListenPacket("udp", s.addr)
	if err != nil {
		s.logger.Error("server: udp listen failed", "addr", s.addr, "err", err)
		return err
	}
	s.pc = pc
	s.listening.Store(true)
	s.logger.Info("server: udp listening", "addr", pc.LocalAddr().String())
	defer s.listening.Store(false)
	defer pc.Close()

	buf := make([]byte, 64*1024)
	for {
		n, src, err := pc.ReadFrom(buf)
		if err != nil {
			if errors.Is(err, net.ErrClosed) {
				return nil
			}
			// If caller cancelled, treat as graceful.
			if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) {
				return err
			}
			s.logger.Warn("server: udp read failed", "err", err)
			return err
		}
		if n <= 0 {
			continue
		}
		if s.h != nil {
			s.h.HandlePacket(ctx, pc, src, buf[:n])
		}
	}
}

func (s *UDPServer) Shutdown(_ context.Context) error {
	if s.pc != nil {
		_ = s.pc.Close()
	}
	return nil
}
