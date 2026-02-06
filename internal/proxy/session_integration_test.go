package proxy

import (
	"bytes"
	"context"
	"io"
	"net"
	"testing"
	"time"

	"prism/internal/config"
	"prism/internal/protocol"
	"prism/internal/router"
	"prism/pkg/mcproto"
)

type mockDialer struct {
	called chan string
	conn   net.Conn
}

func (d *mockDialer) DialContext(_ context.Context, _ string, address string) (net.Conn, error) {
	d.called <- address
	return d.conn, nil
}

func buildHandshakePacket(host string, port uint16, protoVer int32, nextState int32) []byte {
	var payload bytes.Buffer
	_, _ = mcproto.WriteVarInt(&payload, 0) // packet id
	_, _ = mcproto.WriteVarInt(&payload, protoVer)
	_, _ = mcproto.WriteString(&payload, host)
	_, _ = mcproto.WriteUShort(&payload, port)
	_, _ = mcproto.WriteVarInt(&payload, nextState)

	var out bytes.Buffer
	_, _ = mcproto.WriteVarInt(&out, int32(payload.Len()))
	_, _ = out.Write(payload.Bytes())
	return out.Bytes()
}

func TestSessionHandler_ForwardsHandshakeAndPayload(t *testing.T) {
	clientConn, serverConn := net.Pipe()
	upConn, backendConn := net.Pipe()
	defer clientConn.Close()
	defer backendConn.Close()

	dial := &mockDialer{called: make(chan string, 1), conn: upConn}
	r := router.NewRouter(map[string]string{"play.example.com": "127.0.0.1:25566"})
	parser := protocol.NewMinecraftHostParser()
	bridge := NewProxyBridge(ProxyBridgeOptions{})

	h := NewSessionHandler(SessionHandlerOptions{
		Parser:   parser,
		Resolver: r,
		Dialer:   dial,
		Bridge:   bridge,
		Timeouts: config.Timeouts{HandshakeTimeout: 2 * time.Second},
		MaxHeaderBytes: 64 * 1024,
	})

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	go h.Handle(ctx, serverConn)

	handshake := buildHandshakePacket("play.example.com", 25565, 763, 1)
	payload := []byte("hello")
	want := append(append([]byte(nil), handshake...), payload...)

	backendGotCh := make(chan []byte, 1)
	backendErrCh := make(chan error, 1)
	go func() {
		got := make([]byte, len(want))
		_, err := io.ReadFull(backendConn, got)
		if err != nil {
			backendErrCh <- err
			return
		}
		backendGotCh <- got
	}()

	// net.Pipe has backpressure: writing handshake+payload in one go can block until the proxying
	// goroutines start consuming the post-handshake bytes. Write the handshake first.
	if _, err := clientConn.Write(handshake); err != nil {
		t.Fatalf("client write handshake: %v", err)
	}

	select {
	case addr := <-dial.called:
		if addr != "127.0.0.1:25566" {
			t.Fatalf("dial addr: want %q got %q", "127.0.0.1:25566", addr)
		}
	case <-time.After(2 * time.Second):
		t.Fatalf("dial not called")
	}

	if _, err := clientConn.Write(payload); err != nil {
		t.Fatalf("client write payload: %v", err)
	}
	_ = clientConn.Close()

	select {
	case err := <-backendErrCh:
		t.Fatalf("backend read: %v", err)
	case got := <-backendGotCh:
		if !bytes.Equal(got, want) {
			t.Fatalf("forwarded bytes mismatch")
		}
	case <-time.After(2 * time.Second):
		t.Fatalf("backend did not receive bytes")
	}
}
