package tunnel

import (
	"context"
	"errors"
	"io"
	"net"
	"testing"
	"time"
)

func TestTunnelTCPForwarding(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	// Backend echo server.
	backendLn, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("listen backend: %v", err)
	}
	defer backendLn.Close()
	go func() {
		for {
			c, err := backendLn.Accept()
			if err != nil {
				return
			}
			go func(conn net.Conn) {
				defer conn.Close()
				buf := make([]byte, 32*1024)
				for {
					n, rerr := conn.Read(buf)
					if n > 0 {
						_, _ = conn.Write(buf[:n])
					}
					if rerr != nil {
						return
					}
				}
			}(c)
		}
	}()

	mgr := NewManager(nil)
	srv, err := NewServer(ServerOptions{
		Enabled:    true,
		ListenAddr: "127.0.0.1:0",
		Transport:  "tcp",
		AuthToken:  "secret",
		Manager:    mgr,
	})
	if err != nil {
		t.Fatalf("new server: %v", err)
	}

	serverDone := make(chan error, 1)
	go func() {
		serverDone <- srv.ListenAndServe(ctx)
	}()

	// Wait for server to bind.
	deadline := time.Now().Add(2 * time.Second)
	for srv.Addr() == nil {
		if time.Now().After(deadline) {
			t.Fatalf("server did not start listening")
		}
		time.Sleep(10 * time.Millisecond)
	}

	client, err := NewClient(ClientOptions{
		ServerAddr: srv.Addr().String(),
		Transport:  "tcp",
		AuthToken:  "secret",
		Services: []RegisteredService{{
			Name:      "svc",
			LocalAddr: backendLn.Addr().String(),
		}},
		DialTimeout: 2 * time.Second,
	})
	if err != nil {
		t.Fatalf("new client: %v", err)
	}

	clientDone := make(chan error, 1)
	go func() {
		clientDone <- client.Run(ctx)
	}()

	// Wait for service registration.
	deadline = time.Now().Add(3 * time.Second)
	for !mgr.HasService("svc") {
		if time.Now().After(deadline) {
			t.Fatalf("service was not registered")
		}
		time.Sleep(10 * time.Millisecond)
	}

	conn, err := mgr.DialService(ctx, "svc")
	if err != nil {
		t.Fatalf("dial service: %v", err)
	}
	defer conn.Close()
	_ = conn.SetDeadline(time.Now().Add(2 * time.Second))

	payload := []byte("hello over tunnel")
	if _, err := conn.Write(payload); err != nil {
		t.Fatalf("write: %v", err)
	}
	got := make([]byte, len(payload))
	if _, err := io.ReadFull(conn, got); err != nil {
		t.Fatalf("read: %v", err)
	}
	if string(got) != string(payload) {
		t.Fatalf("unexpected echo: %q", string(got))
	}

	// Shutdown.
	cancel()
	_ = srv.Shutdown(context.Background())

	select {
	case err := <-serverDone:
		if err != nil && !errors.Is(err, context.Canceled) {
			// ctx is canceled as part of shutdown.
			t.Fatalf("server returned error: %v", err)
		}
	case <-time.After(2 * time.Second):
		t.Fatalf("server did not exit")
	}

	select {
	case <-clientDone:
		// client likely returns context.Canceled
	case <-time.After(2 * time.Second):
		t.Fatalf("client did not exit")
	}
}
