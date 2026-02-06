package protocol

import (
	"bytes"
	"testing"

	"prism/pkg/mcproto"
)

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

func TestMinecraftHandshakeDecode(t *testing.T) {
	dec := NewMinecraftHandshakeDecoder()
	data := buildHandshakePacket("play.example.com", 25565, 763, 1)
	meta, err := dec.Decode(bytes.NewReader(data))
	if err != nil {
		t.Fatalf("Decode: %v", err)
	}
	if meta.Host != "play.example.com" {
		t.Fatalf("Host: want %q got %q", "play.example.com", meta.Host)
	}
	if meta.Port != 25565 {
		t.Fatalf("Port: want %d got %d", 25565, meta.Port)
	}
}
