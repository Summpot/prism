package proxy

import (
	"encoding/hex"
	"net"
	"testing"
)

func TestBuildProxyV2HeaderIPv4(t *testing.T) {
	src := &net.TCPAddr{IP: net.IPv4(1, 2, 3, 4), Port: 1234}
	dst := &net.TCPAddr{IP: net.IPv4(5, 6, 7, 8), Port: 25565}
	h, err := BuildProxyV2Header(src, dst)
	if err != nil {
		t.Fatalf("BuildProxyV2Header: %v", err)
	}
	// Fixed size: 16 header + 12 address block = 28 bytes
	if len(h) != 28 {
		t.Fatalf("len: want 28 got %d (%s)", len(h), hex.EncodeToString(h))
	}
	// Check signature prefix.
	sigHex := "0d0a0d0a000d0a515549540a"
	if hex.EncodeToString(h[:12]) != sigHex {
		t.Fatalf("signature mismatch")
	}
}
