package protocol

import (
	"encoding/binary"
	"testing"
)

func buildTLSClientHelloWithSNI(host string) []byte {
	// Build server_name extension.
	hostBytes := []byte(host)
	name := make([]byte, 0, 3+len(hostBytes))
	name = append(name, 0x00) // host_name
	name = binary.BigEndian.AppendUint16(name, uint16(len(hostBytes)))
	name = append(name, hostBytes...)

	sni := make([]byte, 0, 2+len(name))
	sni = binary.BigEndian.AppendUint16(sni, uint16(len(name)))
	sni = append(sni, name...)

	ext := make([]byte, 0, 4+len(sni))
	ext = binary.BigEndian.AppendUint16(ext, 0x0000)            // extension type
	ext = binary.BigEndian.AppendUint16(ext, uint16(len(sni)))  // extension length
	ext = append(ext, sni...)

	exts := make([]byte, 0, 2+len(ext))
	exts = binary.BigEndian.AppendUint16(exts, uint16(len(ext)))
	exts = append(exts, ext...)

	// ClientHello body.
	ch := make([]byte, 0, 2+32+1+2+2+1+1+len(exts))
	ch = append(ch, 0x03, 0x03)          // client_version TLS 1.2
	ch = append(ch, make([]byte, 32)...) // random
	ch = append(ch, 0x00)                // session_id_len
	ch = append(ch, 0x00, 0x02)          // cipher_suites_len
	ch = append(ch, 0x00, 0x2f)          // TLS_RSA_WITH_AES_128_CBC_SHA
	ch = append(ch, 0x01)                // compression_methods_len
	ch = append(ch, 0x00)                // null compression
	ch = append(ch, exts...)             // extensions

	// Handshake message wrapper.
	hs := make([]byte, 0, 4+len(ch))
	hs = append(hs, 0x01) // client_hello
	// length: 3 bytes
	l := len(ch)
	hs = append(hs, byte(l>>16), byte(l>>8), byte(l))
	hs = append(hs, ch...)

	// TLS record wrapper.
	rec := make([]byte, 0, 5+len(hs))
	rec = append(rec, 0x16, 0x03, 0x01) // handshake, TLS 1.0 record version
	rec = binary.BigEndian.AppendUint16(rec, uint16(len(hs)))
	rec = append(rec, hs...)
	return rec
}

func TestTLSSNIHostParser_Parse(t *testing.T) {
	p := NewTLSSNIHostParser()
	data := buildTLSClientHelloWithSNI("play.example.com")

	got, err := p.Parse(data)
	if err != nil {
		t.Fatalf("Parse: %v", err)
	}
	if got != "play.example.com" {
		t.Fatalf("host: want %q got %q", "play.example.com", got)
	}
}

func TestTLSSNIHostParser_IncrementalNeedMore(t *testing.T) {
	p := NewTLSSNIHostParser()
	data := buildTLSClientHelloWithSNI("play.example.com")

	for i := 0; i < len(data)-1; i++ {
		_, err := p.Parse(data[:i])
		if err == nil {
			t.Fatalf("expected need-more at prefix %d", i)
		}
		if err != ErrNeedMoreData {
			// Many prefixes are still a definite TLS record header and thus should be need-more.
			// Once enough bytes are present, it will succeed.
			if err == ErrNoMatch {
				t.Fatalf("unexpected no-match at prefix %d", i)
			}
		}
	}

	got, err := p.Parse(data)
	if err != nil {
		t.Fatalf("Parse(full): %v", err)
	}
	if got != "play.example.com" {
		t.Fatalf("host: want %q got %q", "play.example.com", got)
	}
}
