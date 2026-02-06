package proxy

import (
	"bytes"
	"encoding/binary"
	"errors"
	"fmt"
	"net"
)

// PROXY protocol v2 specification: https://www.haproxy.org/download/2.9/doc/proxy-protocol.txt

var (
	proxyV2Sig = []byte{0x0d, 0x0a, 0x0d, 0x0a, 0x00, 0x0d, 0x0a, 0x51, 0x55, 0x49, 0x54, 0x0a}
)

func BuildProxyV2Header(src, dst *net.TCPAddr) ([]byte, error) {
	if src == nil || dst == nil {
		return nil, errors.New("proxyproto: nil addr")
	}

	fam := byte(0x00)
	addrLen := 0
	srcIP := src.IP
	dstIP := dst.IP

	if srcIP.To4() != nil && dstIP.To4() != nil {
		fam = 0x11 // INET + STREAM
		addrLen = 12
		srcIP = srcIP.To4()
		dstIP = dstIP.To4()
	} else if srcIP.To16() != nil && dstIP.To16() != nil {
		fam = 0x21 // INET6 + STREAM
		addrLen = 36
		srcIP = srcIP.To16()
		dstIP = dstIP.To16()
	} else {
		return nil, fmt.Errorf("proxyproto: mismatched ip families: src=%v dst=%v", src.IP, dst.IP)
	}

	buf := bytes.NewBuffer(make([]byte, 0, 16+addrLen))
	buf.Write(proxyV2Sig)
	buf.WriteByte(0x21) // v2 + PROXY command
	buf.WriteByte(fam)
	_ = binary.Write(buf, binary.BigEndian, uint16(addrLen))
	buf.Write(srcIP)
	buf.Write(dstIP)
	_ = binary.Write(buf, binary.BigEndian, uint16(src.Port))
	_ = binary.Write(buf, binary.BigEndian, uint16(dst.Port))

	return buf.Bytes(), nil
}
