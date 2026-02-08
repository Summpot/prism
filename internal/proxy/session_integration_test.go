package proxy

import (
	"bytes"
	"context"
	"encoding/binary"
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

func buildStatusRequestPacket() []byte {
	var payload bytes.Buffer
	_, _ = mcproto.WriteVarInt(&payload, 0) // packet id
	var out bytes.Buffer
	_, _ = mcproto.WriteVarInt(&out, int32(payload.Len()))
	_, _ = out.Write(payload.Bytes())
	return out.Bytes()
}

func buildStatusResponsePacket(json string) []byte {
	var payload bytes.Buffer
	_, _ = mcproto.WriteVarInt(&payload, 0) // packet id
	_, _ = mcproto.WriteString(&payload, json)
	var out bytes.Buffer
	_, _ = mcproto.WriteVarInt(&out, int32(payload.Len()))
	_, _ = out.Write(payload.Bytes())
	return out.Bytes()
}

func buildPingPacket(v int64) []byte {
	var payload bytes.Buffer
	_, _ = mcproto.WriteVarInt(&payload, 1) // packet id
	var b [8]byte
	binary.BigEndian.PutUint64(b[:], uint64(v))
	_, _ = payload.Write(b[:])

	var out bytes.Buffer
	_, _ = mcproto.WriteVarInt(&out, int32(payload.Len()))
	_, _ = out.Write(payload.Bytes())
	return out.Bytes()
}

func readFrame(t *testing.T, r io.Reader) ([]byte, int32) {
	t.Helper()
	ln, _, err := mcproto.ReadVarInt(r)
	if err != nil {
		t.Fatalf("ReadVarInt(len): %v", err)
	}
	if ln < 0 {
		t.Fatalf("negative len")
	}
	payload := make([]byte, int(ln))
	if _, err := io.ReadFull(r, payload); err != nil {
		t.Fatalf("ReadFull(payload): %v", err)
	}
	pid, _, err := mcproto.ReadVarInt(bytes.NewReader(payload))
	if err != nil {
		t.Fatalf("ReadVarInt(pid): %v", err)
	}
	return payload, pid
}

func TestSessionHandler_ForwardsHandshakeAndPayload(t *testing.T) {
	clientConn, serverConn := net.Pipe()
	upConn, backendConn := net.Pipe()
	defer clientConn.Close()
	defer backendConn.Close()

	dial := &mockDialer{called: make(chan string, 1), conn: upConn}
	r := router.NewRouter([]router.Route{{Host: []string{"play.example.com"}, Upstreams: []string{"127.0.0.1:25566"}}})
	parser := protocol.NewMinecraftHostParser()
	bridge := NewProxyBridge(ProxyBridgeOptions{})

	h := NewSessionHandler(SessionHandlerOptions{
		Parser:              parser,
		Resolver:            r,
		Dialer:              dial,
		Bridge:              bridge,
		Timeouts:            config.Timeouts{HandshakeTimeout: 2 * time.Second},
		MaxHeaderBytes:      64 * 1024,
		DefaultUpstreamPort: 25565,
	})

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	go h.Handle(ctx, serverConn)

	handshake := buildHandshakePacket("play.example.com", 25565, 763, 2)
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

func TestSessionHandler_AppendsPortWhenMissing_UsesHandshakePort(t *testing.T) {
	clientConn, serverConn := net.Pipe()
	upConn, backendConn := net.Pipe()
	defer clientConn.Close()
	defer backendConn.Close()

	dial := &mockDialer{called: make(chan string, 1), conn: upConn}
	// Upstream intentionally has no port.
	r := router.NewRouter([]router.Route{{Host: []string{"play.example.com"}, Upstreams: []string{"backend.example.com"}}})
	parser := protocol.NewMinecraftHostParser()
	bridge := NewProxyBridge(ProxyBridgeOptions{})

	h := NewSessionHandler(SessionHandlerOptions{
		Parser:              parser,
		Resolver:            r,
		Dialer:              dial,
		Bridge:              bridge,
		Timeouts:            config.Timeouts{HandshakeTimeout: 2 * time.Second},
		MaxHeaderBytes:      64 * 1024,
		DefaultUpstreamPort: 12345, // should be overridden by handshake port
	})

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	go h.Handle(ctx, serverConn)

	handshake := buildHandshakePacket("play.example.com", 25565, 763, 2)
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

	if _, err := clientConn.Write(handshake); err != nil {
		t.Fatalf("client write handshake: %v", err)
	}

	select {
	case addr := <-dial.called:
		if addr != "backend.example.com:25565" {
			t.Fatalf("dial addr: want %q got %q", "backend.example.com:25565", addr)
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

func TestSessionHandler_StatusResponseCaching(t *testing.T) {
	cache := NewStatusCache()

	// Router with ping cache enabled.
	r := router.NewRouter([]router.Route{{
		Host:         []string{"play.example.com"},
		Upstreams:    []string{"127.0.0.1:25566"},
		CachePingTTL: 5 * time.Second,
	}})
	parser := protocol.NewMinecraftHostParser()
	bridge := NewProxyBridge(ProxyBridgeOptions{})

	// First ping: should dial upstream and populate cache.
	{
		clientConn, serverConn := net.Pipe()
		upConn, backendConn := net.Pipe()
		defer clientConn.Close()
		defer backendConn.Close()

		dial := &mockDialer{called: make(chan string, 1), conn: upConn}
		h := NewSessionHandler(SessionHandlerOptions{
			Parser:              parser,
			Resolver:            r,
			Dialer:              dial,
			Bridge:              bridge,
			StatusCache:         cache,
			Timeouts:            config.Timeouts{HandshakeTimeout: 2 * time.Second},
			MaxHeaderBytes:      64 * 1024,
			DefaultUpstreamPort: 25565,
		})

		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		go h.Handle(ctx, serverConn)

		handshake := buildHandshakePacket("play.example.com", 25565, 763, 1)
		statusReq := buildStatusRequestPacket()
		ping := buildPingPacket(42)

		backendWant := append(append([]byte(nil), handshake...), statusReq...)
		backendGotCh := make(chan []byte, 1)
		go func() {
			got := make([]byte, len(backendWant))
			_, _ = io.ReadFull(backendConn, got)
			backendGotCh <- got
			// Reply with a status response.
			_, _ = backendConn.Write(buildStatusResponsePacket(`{"version":{"name":"x","protocol":763},"players":{"max":0,"online":0},"description":"hi"}`))
		}()

		// Send handshake, status request, and ping.
		if _, err := clientConn.Write(handshake); err != nil {
			t.Fatalf("client write handshake: %v", err)
		}
		if _, err := clientConn.Write(statusReq); err != nil {
			t.Fatalf("client write status req: %v", err)
		}

		select {
		case addr := <-dial.called:
			if addr != "127.0.0.1:25566" {
				t.Fatalf("dial addr: want %q got %q", "127.0.0.1:25566", addr)
			}
		case <-time.After(2 * time.Second):
			t.Fatalf("dial not called")
		}

		select {
		case got := <-backendGotCh:
			if !bytes.Equal(got, backendWant) {
				t.Fatalf("backend got mismatch")
			}
		case <-time.After(2 * time.Second):
			t.Fatalf("backend did not receive handshake+status")
		}

		// Client should receive a status response and a pong.
		payload, pid := readFrame(t, clientConn)
		if pid != 0 {
			t.Fatalf("status pid=%d want 0", pid)
		}
		// Skip packet id, then string.
		br := bytes.NewReader(payload)
		_, _, _ = mcproto.ReadVarInt(br)
		status, _, err := mcproto.ReadString(br)
		if err != nil {
			t.Fatalf("ReadString(status): %v", err)
		}
		if !bytes.Contains([]byte(status), []byte("\"description\"")) {
			t.Fatalf("unexpected status: %s", status)
		}

		// Send ping only after we have read the status response.
		if _, err := clientConn.Write(ping); err != nil {
			t.Fatalf("client write ping: %v", err)
		}

		payload, pid = readFrame(t, clientConn)
		if pid != 1 {
			t.Fatalf("pong pid=%d want 1", pid)
		}
		br = bytes.NewReader(payload)
		_, _, _ = mcproto.ReadVarInt(br)
		var b [8]byte
		if _, err := io.ReadFull(br, b[:]); err != nil {
			t.Fatalf("ReadFull(pong long): %v", err)
		}
		if got := int64(binary.BigEndian.Uint64(b[:])); got != 42 {
			t.Fatalf("pong value=%d want 42", got)
		}
	}

	// Second ping: should serve from cache without dialing.
	{
		clientConn, serverConn := net.Pipe()
		defer clientConn.Close()

		dial := &mockDialer{called: make(chan string, 1), conn: nil}
		h := NewSessionHandler(SessionHandlerOptions{
			Parser:              parser,
			Resolver:            r,
			Dialer:              dial,
			Bridge:              bridge,
			StatusCache:         cache,
			Timeouts:            config.Timeouts{HandshakeTimeout: 2 * time.Second},
			MaxHeaderBytes:      64 * 1024,
			DefaultUpstreamPort: 25565,
		})

		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		go h.Handle(ctx, serverConn)

		handshake := buildHandshakePacket("play.example.com", 25565, 763, 1)
		statusReq := buildStatusRequestPacket()
		ping := buildPingPacket(7)

		if _, err := clientConn.Write(handshake); err != nil {
			t.Fatalf("client write handshake: %v", err)
		}
		if _, err := clientConn.Write(statusReq); err != nil {
			t.Fatalf("client write status req: %v", err)
		}

		select {
		case addr := <-dial.called:
			t.Fatalf("unexpected dial to %q", addr)
		case <-time.After(200 * time.Millisecond):
			// ok
		}

		_, pid := readFrame(t, clientConn)
		if pid != 0 {
			t.Fatalf("status pid=%d want 0", pid)
		}
		// Send ping only after we have read the status response.
		if _, err := clientConn.Write(ping); err != nil {
			t.Fatalf("client write ping: %v", err)
		}
		_, pid = readFrame(t, clientConn)
		if pid != 1 {
			t.Fatalf("pong pid=%d want 1", pid)
		}
	}
}
