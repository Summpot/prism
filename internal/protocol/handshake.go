package protocol

import (
	"bytes"
	"fmt"
	"io"

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
