package protocol

import (
	"encoding/binary"
	"strings"
)

// TLSSNIHostParser extracts the first SNI hostname from a TLS ClientHello.
//
// This parser only supports the standard record framing and the ClientHello
// extension layout needed to reach server_name.
type TLSSNIHostParser struct{}

func NewTLSSNIHostParser() *TLSSNIHostParser { return &TLSSNIHostParser{} }

func (p *TLSSNIHostParser) Name() string { return "tls_sni" }

func (p *TLSSNIHostParser) Parse(prelude []byte) (string, error) {
	// TLS record header: type(1) version(2) length(2)
	if len(prelude) < 5 {
		return "", ErrNeedMoreData
	}
	if prelude[0] != 0x16 { // handshake
		return "", ErrNoMatch
	}
	// Accept TLS 1.0..1.3 record versions (0x0301..0x0304)
	vMaj, vMin := prelude[1], prelude[2]
	if vMaj != 0x03 || vMin < 0x01 || vMin > 0x04 {
		return "", ErrNoMatch
	}
	recLen := int(binary.BigEndian.Uint16(prelude[3:5]))
	if recLen <= 0 {
		return "", ErrNoMatch
	}
	if len(prelude) < 5+recLen {
		return "", ErrNeedMoreData
	}
	rec := prelude[5 : 5+recLen]

	// Handshake message: msg_type(1) length(3)
	if len(rec) < 4 {
		return "", ErrNeedMoreData
	}
	if rec[0] != 0x01 { // client_hello
		return "", ErrNoMatch
	}
	hsLen := int(rec[1])<<16 | int(rec[2])<<8 | int(rec[3])
	if hsLen <= 0 {
		return "", ErrNoMatch
	}
	if len(rec) < 4+hsLen {
		return "", ErrNeedMoreData
	}
	ch := rec[4 : 4+hsLen]

	i := 0
	// client_version(2) + random(32)
	if len(ch) < 2+32 {
		return "", ErrNeedMoreData
	}
	i += 2 + 32

	// session_id
	if i+1 > len(ch) {
		return "", ErrNeedMoreData
	}
	sidLen := int(ch[i])
	i += 1
	if i+sidLen > len(ch) {
		return "", ErrNeedMoreData
	}
	i += sidLen

	// cipher_suites
	if i+2 > len(ch) {
		return "", ErrNeedMoreData
	}
	csLen := int(binary.BigEndian.Uint16(ch[i : i+2]))
	i += 2
	if csLen < 2 || csLen%2 != 0 {
		return "", ErrNoMatch
	}
	if i+csLen > len(ch) {
		return "", ErrNeedMoreData
	}
	i += csLen

	// compression_methods
	if i+1 > len(ch) {
		return "", ErrNeedMoreData
	}
	cmLen := int(ch[i])
	i += 1
	if i+cmLen > len(ch) {
		return "", ErrNeedMoreData
	}
	i += cmLen

	// extensions
	if i == len(ch) {
		return "", ErrNoMatch
	}
	if i+2 > len(ch) {
		return "", ErrNeedMoreData
	}
	extLen := int(binary.BigEndian.Uint16(ch[i : i+2]))
	i += 2
	if extLen < 0 || i+extLen > len(ch) {
		return "", ErrNeedMoreData
	}
	ext := ch[i : i+extLen]

	j := 0
	for j+4 <= len(ext) {
		extType := binary.BigEndian.Uint16(ext[j : j+2])
		extDataLen := int(binary.BigEndian.Uint16(ext[j+2 : j+4]))
		j += 4
		if j+extDataLen > len(ext) {
			return "", ErrNoMatch
		}
		if extType != 0x0000 {
			j += extDataLen
			continue
		}
		// server_name extension
		data := ext[j : j+extDataLen]
		if len(data) < 2 {
			return "", ErrNoMatch
		}
		listLen := int(binary.BigEndian.Uint16(data[0:2]))
		if 2+listLen > len(data) {
			return "", ErrNoMatch
		}
		k := 2
		end := 2 + listLen
		for k+3 <= end {
			nameType := data[k]
			nameLen := int(binary.BigEndian.Uint16(data[k+1 : k+3]))
			k += 3
			if k+nameLen > end {
				return "", ErrNoMatch
			}
			if nameType == 0x00 {
				host := string(data[k : k+nameLen])
				host = strings.TrimSpace(strings.ToLower(host))
				if host == "" {
					return "", ErrNoMatch
				}
				return host, nil
			}
			k += nameLen
		}
		return "", ErrNoMatch
	}

	return "", ErrNoMatch
}

var _ HostParser = (*TLSSNIHostParser)(nil)
