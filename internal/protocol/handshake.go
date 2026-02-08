package protocol

import (
	"bytes"
	"fmt"
	"io"
	"strings"

	"prism/pkg/mcproto"
)

type HandshakeMetadata struct {
	Host            string
	Port            uint16
	ProtocolVersion int32
	NextState       int32
}

type HandshakeDecoder interface {
	Decode(r io.Reader) (*HandshakeMetadata, error)
}

type MinecraftHandshakeDecoder struct{}

func NewMinecraftHandshakeDecoder() *MinecraftHandshakeDecoder {
	return &MinecraftHandshakeDecoder{}
}

// Decode reads and parses a Minecraft handshake packet from r.
// It expects the packet framing: [packet_length VarInt][packet_id VarInt=0][protocol_version VarInt]
// [server_address String][server_port UnsignedShort][next_state VarInt].
func (d *MinecraftHandshakeDecoder) Decode(r io.Reader) (*HandshakeMetadata, error) {
	packetLen, _, err := mcproto.ReadVarInt(r)
	if err != nil {
		return nil, err
	}
	if packetLen <= 0 {
		return nil, fmt.Errorf("protocol: invalid packet length %d", packetLen)
	}

	packet := make([]byte, int(packetLen))
	if _, err := io.ReadFull(r, packet); err != nil {
		return nil, err
	}

	br := bytes.NewReader(packet)
	packetID, _, err := mcproto.ReadVarInt(br)
	if err != nil {
		return nil, err
	}
	if packetID != 0 {
		return nil, fmt.Errorf("protocol: expected handshake packet id 0, got %d", packetID)
	}

	protoVer, _, err := mcproto.ReadVarInt(br)
	if err != nil {
		return nil, err
	}
	host, _, err := mcproto.ReadString(br)
	if err != nil {
		return nil, err
	}
	port, _, err := mcproto.ReadUShort(br)
	if err != nil {
		return nil, err
	}
	nextState, _, err := mcproto.ReadVarInt(br)
	if err != nil {
		return nil, err
	}

	return &HandshakeMetadata{
		Host:            host,
		Port:            port,
		ProtocolVersion: protoVer,
		NextState:       nextState,
	}, nil
}

// TryParseMinecraftHandshakePort attempts to parse the server port from a
// captured Minecraft handshake prelude.
//
// This is intended for routing cases where the configured upstream address is
// a bare host without a port (e.g. "backend.example.com").
//
// Safety: maxPacketLen is enforced to avoid large allocations or pathological
// reads when prelude does not actually contain a Minecraft handshake.
func TryParseMinecraftHandshakePort(prelude []byte, maxPacketLen int) (uint16, bool) {
	_, port, ok := TryParseMinecraftHandshakeHostPort(prelude, maxPacketLen, 0)
	return port, ok
}

// TryParseMinecraftHandshakeHostPort attempts to parse both the host and port
// from a captured Minecraft handshake prelude.
//
// The returned host is normalized (lowercased, trimmed) to match routing.
func TryParseMinecraftHandshakeHostPort(prelude []byte, maxPacketLen int, maxHostLen int) (string, uint16, bool) {
	if len(prelude) == 0 {
		return "", 0, false
	}
	if maxPacketLen <= 0 {
		maxPacketLen = 256 * 1024
	}
	if maxHostLen <= 0 {
		maxHostLen = 255
	}

	br := bytes.NewReader(prelude)
	packetLen32, nLen, err := mcproto.ReadVarInt(br)
	if err != nil {
		return "", 0, false
	}
	if packetLen32 <= 0 {
		return "", 0, false
	}
	packetLen := int(packetLen32)
	if packetLen > maxPacketLen {
		return "", 0, false
	}
	if nLen+packetLen > len(prelude) {
		return "", 0, false
	}

	packet := prelude[nLen : nLen+packetLen]
	pbr := bytes.NewReader(packet)

	packetID, _, err := mcproto.ReadVarInt(pbr)
	if err != nil || packetID != 0 {
		return "", 0, false
	}

	// proto version
	if _, _, err := mcproto.ReadVarInt(pbr); err != nil {
		return "", 0, false
	}

	// server_address String: [len VarInt][bytes]
	addrLen32, _, err := mcproto.ReadVarInt(pbr)
	if err != nil {
		return "", 0, false
	}
	if addrLen32 < 0 {
		return "", 0, false
	}
	addrLen := int(addrLen32)
	if addrLen > maxHostLen {
		return "", 0, false
	}
	if addrLen > pbr.Len() {
		return "", 0, false
	}

	// Host bytes start at the current reader position.
	hostStart := len(packet) - pbr.Len()
	if hostStart < 0 || hostStart+addrLen > len(packet) {
		return "", 0, false
	}
	hostBytes := packet[hostStart : hostStart+addrLen]
	if _, err := pbr.Seek(int64(addrLen), io.SeekCurrent); err != nil {
		return "", 0, false
	}

	port, _, err := mcproto.ReadUShort(pbr)
	if err != nil {
		return "", 0, false
	}

	host := strings.TrimSpace(strings.ToLower(string(hostBytes)))
	if host == "" {
		return "", 0, false
	}

	return host, port, true
}

// TryParseMinecraftHandshakeMetadata attempts to parse a full Minecraft handshake
// from a captured prelude. It returns the parsed metadata, the total handshake
// frame length in bytes (including the length VarInt), and whether parsing succeeded.
//
// Safety: maxPacketLen and maxHostLen are enforced to avoid large allocations
// when the prelude is not actually a Minecraft handshake.
func TryParseMinecraftHandshakeMetadata(prelude []byte, maxPacketLen int, maxHostLen int) (HandshakeMetadata, int, bool) {
	if len(prelude) == 0 {
		return HandshakeMetadata{}, 0, false
	}
	if maxPacketLen <= 0 {
		maxPacketLen = 256 * 1024
	}
	if maxHostLen <= 0 {
		maxHostLen = 255
	}

	br := bytes.NewReader(prelude)
	packetLen32, nLen, err := mcproto.ReadVarInt(br)
	if err != nil {
		return HandshakeMetadata{}, 0, false
	}
	if packetLen32 <= 0 {
		return HandshakeMetadata{}, 0, false
	}
	packetLen := int(packetLen32)
	if packetLen > maxPacketLen {
		return HandshakeMetadata{}, 0, false
	}
	if nLen+packetLen > len(prelude) {
		return HandshakeMetadata{}, 0, false
	}

	packet := prelude[nLen : nLen+packetLen]
	pbr := bytes.NewReader(packet)
	packetID, _, err := mcproto.ReadVarInt(pbr)
	if err != nil || packetID != 0 {
		return HandshakeMetadata{}, 0, false
	}
	protoVer, _, err := mcproto.ReadVarInt(pbr)
	if err != nil {
		return HandshakeMetadata{}, 0, false
	}

	// server_address String: [len VarInt][bytes]
	addrLen32, _, err := mcproto.ReadVarInt(pbr)
	if err != nil {
		return HandshakeMetadata{}, 0, false
	}
	if addrLen32 < 0 {
		return HandshakeMetadata{}, 0, false
	}
	addrLen := int(addrLen32)
	if addrLen > maxHostLen {
		return HandshakeMetadata{}, 0, false
	}
	if addrLen > pbr.Len() {
		return HandshakeMetadata{}, 0, false
	}
	addrStart := len(packet) - pbr.Len()
	if addrStart < 0 || addrStart+addrLen > len(packet) {
		return HandshakeMetadata{}, 0, false
	}
	hostBytes := packet[addrStart : addrStart+addrLen]
	if _, err := pbr.Seek(int64(addrLen), io.SeekCurrent); err != nil {
		return HandshakeMetadata{}, 0, false
	}

	port, _, err := mcproto.ReadUShort(pbr)
	if err != nil {
		return HandshakeMetadata{}, 0, false
	}
	nextState, _, err := mcproto.ReadVarInt(pbr)
	if err != nil {
		return HandshakeMetadata{}, 0, false
	}

	host := strings.TrimSpace(strings.ToLower(string(hostBytes)))
	if host == "" {
		return HandshakeMetadata{}, 0, false
	}

	return HandshakeMetadata{Host: host, Port: port, ProtocolVersion: protoVer, NextState: nextState}, nLen + packetLen, true
}
