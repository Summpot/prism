package protocol

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"strings"

	"prism/pkg/mcproto"
)

// MinecraftHostParser extracts the server address from a Minecraft handshake packet.
//
// It expects the packet framing: [packet_length VarInt][packet_id VarInt=0][protocol_version VarInt]
// [server_address String][server_port UnsignedShort][next_state VarInt].
//
// This is used only to route the connection; all captured bytes are forwarded upstream unchanged.
type MinecraftHostParser struct {
	// MaxPacketLen is a safety limit for the decoded packet length.
	// If <= 0, a conservative default is used.
	MaxPacketLen int
}

func NewMinecraftHostParser() *MinecraftHostParser { return &MinecraftHostParser{} }

func (p *MinecraftHostParser) Name() string { return "minecraft_handshake" }

func (p *MinecraftHostParser) Parse(prelude []byte) (string, error) {
	if len(prelude) == 0 {
		return "", ErrNeedMoreData
	}

	br := bytes.NewReader(prelude)
	packetLen32, nLen, err := mcproto.ReadVarInt(br)
	if err != nil {
		if errors.Is(err, mcproto.ErrVarIntEOF) {
			return "", ErrNeedMoreData
		}
		return "", ErrNoMatch
	}
	if packetLen32 <= 0 {
		return "", ErrNoMatch
	}

	maxPacket := p.MaxPacketLen
	if maxPacket <= 0 {
		maxPacket = 256 * 1024
	}
	packetLen := int(packetLen32)
	if packetLen > maxPacket {
		return "", ErrNoMatch
	}

	needTotal := nLen + packetLen
	if len(prelude) < needTotal {
		return "", ErrNeedMoreData
	}

	packet := prelude[nLen:needTotal]
	pbr := bytes.NewReader(packet)
	packetID, _, err := mcproto.ReadVarInt(pbr)
	if err != nil {
		return "", ErrNoMatch
	}
	if packetID != 0 {
		return "", ErrNoMatch
	}

	_, _, err = mcproto.ReadVarInt(pbr) // proto version
	if err != nil {
		return "", ErrNoMatch
	}
	host, _, err := mcproto.ReadString(pbr)
	if err != nil {
		if errors.Is(err, io.ErrUnexpectedEOF) || errors.Is(err, io.EOF) || errors.Is(err, mcproto.ErrVarIntEOF) {
			return "", ErrNeedMoreData
		}
		return "", ErrNoMatch
	}
	if host == "" {
		return "", fmt.Errorf("protocol: minecraft handshake empty host")
	}

	// Normalize to match Router behavior.
	host = strings.TrimSpace(strings.ToLower(host))
	return host, nil
}

var _ HostParser = (*MinecraftHostParser)(nil)
