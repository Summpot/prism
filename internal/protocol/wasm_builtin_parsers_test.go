package protocol

import (
	"context"
	"testing"
)

func TestBuiltinWASMHostParser_MinecraftHandshake_Parse(t *testing.T) {
	p, err := NewBuiltinWASMHostParser(context.Background(), "minecraft_handshake", WASMHostParserOptions{
		Name: "minecraft_handshake",
	})
	if err != nil {
		t.Fatalf("NewBuiltinWASMHostParser: %v", err)
	}
	defer func() { _ = p.Close(context.Background()) }()

	data := buildHandshakePacket("Play.Example.Com", 25565, 763, 1)
	got, err := p.Parse(data)
	if err != nil {
		t.Fatalf("Parse(full): %v", err)
	}
	if got != "play.example.com" {
		t.Fatalf("host: want %q got %q", "play.example.com", got)
	}

	// Incremental prefixes should not be a definite non-match.
	for i := 0; i < len(data)-1; i++ {
		_, err := p.Parse(data[:i])
		if err == nil {
			t.Fatalf("expected error at prefix %d", i)
		}
		if err == ErrNoMatch {
			t.Fatalf("unexpected no-match at prefix %d", i)
		}
	}
}

func TestBuiltinWASMHostParser_TLSSNI_Parse(t *testing.T) {
	p, err := NewBuiltinWASMHostParser(context.Background(), "tls_sni", WASMHostParserOptions{
		Name: "tls_sni",
	})
	if err != nil {
		t.Fatalf("NewBuiltinWASMHostParser: %v", err)
	}
	defer func() { _ = p.Close(context.Background()) }()

	data := buildTLSClientHelloWithSNI("Play.Example.Com")
	got, err := p.Parse(data)
	if err != nil {
		t.Fatalf("Parse(full): %v", err)
	}
	if got != "play.example.com" {
		t.Fatalf("host: want %q got %q", "play.example.com", got)
	}

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
}
