package proxy

import (
	"context"
	"io"
	"log/slog"
	"net"
	"testing"
	"time"

	"prism/internal/server"
)

func TestUDPForwarder_DirectEcho(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	logger := slog.New(slog.NewTextHandler(io.Discard, &slog.HandlerOptions{Level: slog.LevelDebug}))

	// Backend UDP echo server.
	backend, err := net.ListenPacket("udp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("listen backend: %v", err)
	}
	defer backend.Close()
	go func() {
		buf := make([]byte, 64*1024)
		for {
			n, addr, rerr := backend.ReadFrom(buf)
			if rerr != nil {
				return
			}
			_, _ = backend.WriteTo(buf[:n], addr)
		}
	}()

	dialer := NewNetDialer(&NetDialerOptions{Timeout: 2 * time.Second})
	fwd := NewUDPForwarder(UDPForwarderOptions{
		Upstream:    backend.LocalAddr().String(),
		Dialer:      dialer,
		IdleTimeout: 2 * time.Second,
		Logger:      logger,
	})

	udpSrv := server.NewUDPServer("127.0.0.1:0", fwd, logger)
	srvDone := make(chan error, 1)
	go func() { srvDone <- udpSrv.ListenAndServe(ctx) }()

	deadline := time.Now().Add(2 * time.Second)
	for udpSrv.Addr() == nil {
		if time.Now().After(deadline) {
			t.Fatalf("udp server did not start")
		}
		time.Sleep(10 * time.Millisecond)
	}

	c, err := net.Dial("udp", udpSrv.Addr().String())
	if err != nil {
		t.Fatalf("dial udp server: %v", err)
	}
	defer c.Close()
	_ = c.SetDeadline(time.Now().Add(2 * time.Second))

	payload := []byte("ping")
	if _, err := c.Write(payload); err != nil {
		t.Fatalf("write: %v", err)
	}
	buf := make([]byte, 32)
	n, err := c.Read(buf)
	if err != nil {
		t.Fatalf("read: %v", err)
	}
	if string(buf[:n]) != string(payload) {
		t.Fatalf("got %q want %q", string(buf[:n]), string(payload))
	}

	// Shutdown.
	cancel()
	_ = udpSrv.Shutdown(context.Background())
	select {
	case <-srvDone:
	case <-time.After(2 * time.Second):
		t.Fatalf("udp server did not exit")
	}
}
