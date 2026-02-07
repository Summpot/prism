package tunnel

import (
	"bufio"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"strings"

	"prism/pkg/mcproto"
)

const (
	magicRegister = "PRRG" // Prism Reverse Register
	magicProxyTCP = "PRPX" // Prism Reverse Proxy (TCP stream)
	magicProxyUDP = "PRPU" // Prism Reverse Proxy (UDP datagram stream)
	protocolV1    = byte(1)
)

var (
	ErrBadMagic   = errors.New("tunnel: bad magic")
	ErrBadVersion = errors.New("tunnel: unsupported version")
)

type RegisterRequest struct {
	Token    string              `json:"token"`
	Services []RegisteredService `json:"services"`
}

type RegisteredService struct {
	Name string `json:"name"`
	// Proto is one of: tcp, udp. Defaults to tcp if omitted.
	Proto string `json:"proto,omitempty"`
	// LocalAddr is only used by prismc; prisms stores it for debugging.
	LocalAddr string `json:"local_addr"`
	// RouteOnly marks this service as only reachable via routing (tunnel:<service>)
	// and never exposed as a server-side listener.
	//
	// When true, RemoteAddr should be empty.
	RouteOnly bool `json:"route_only,omitempty"`
	// RemoteAddr (optional) requests prisms to open a public listener for this
	// service (frp-like behavior).
	RemoteAddr string `json:"remote_addr,omitempty"`
}

func writeRegisterRequest(w io.Writer, req RegisterRequest) error {
	if _, err := io.WriteString(w, magicRegister); err != nil {
		return err
	}
	if _, err := w.Write([]byte{protocolV1}); err != nil {
		return err
	}

	b, err := json.Marshal(req)
	if err != nil {
		return err
	}
	var lenBuf [4]byte
	binary.BigEndian.PutUint32(lenBuf[:], uint32(len(b)))
	if _, err := w.Write(lenBuf[:]); err != nil {
		return err
	}
	_, err = w.Write(b)
	return err
}

func readRegisterRequest(r io.Reader) (RegisterRequest, error) {
	r = bufio.NewReader(r)
	var hdr [4]byte
	if _, err := io.ReadFull(r, hdr[:]); err != nil {
		return RegisterRequest{}, err
	}
	if string(hdr[:]) != magicRegister {
		return RegisterRequest{}, ErrBadMagic
	}
	var ver [1]byte
	if _, err := io.ReadFull(r, ver[:]); err != nil {
		return RegisterRequest{}, err
	}
	if ver[0] != protocolV1 {
		return RegisterRequest{}, ErrBadVersion
	}
	var lenBuf [4]byte
	if _, err := io.ReadFull(r, lenBuf[:]); err != nil {
		return RegisterRequest{}, err
	}
	n := binary.BigEndian.Uint32(lenBuf[:])
	if n > 1<<20 { // 1 MiB cap
		return RegisterRequest{}, fmt.Errorf("tunnel: register payload too large: %d", n)
	}
	buf := make([]byte, int(n))
	if _, err := io.ReadFull(r, buf); err != nil {
		return RegisterRequest{}, err
	}
	var req RegisterRequest
	if err := json.Unmarshal(buf, &req); err != nil {
		return RegisterRequest{}, err
	}
	for i := range req.Services {
		req.Services[i].Name = strings.TrimSpace(req.Services[i].Name)
		req.Services[i].Proto = strings.TrimSpace(strings.ToLower(req.Services[i].Proto))
		if req.Services[i].Proto == "" {
			req.Services[i].Proto = "tcp"
		}
		req.Services[i].LocalAddr = strings.TrimSpace(req.Services[i].LocalAddr)
		req.Services[i].RemoteAddr = strings.TrimSpace(req.Services[i].RemoteAddr)
		if req.Services[i].RouteOnly {
			// Defensive normalization: route-only services are never auto-exposed.
			req.Services[i].RemoteAddr = ""
		}
	}
	return req, nil
}

type ProxyStreamKind string

const (
	ProxyStreamTCP ProxyStreamKind = "tcp"
	ProxyStreamUDP ProxyStreamKind = "udp"
)

func writeProxyStreamHeader(w io.Writer, service string) error {
	return writeProxyStreamHeaderKind(w, ProxyStreamTCP, service)
}

func writeProxyStreamHeaderKind(w io.Writer, kind ProxyStreamKind, service string) error {
	magic := ""
	switch kind {
	case ProxyStreamTCP:
		magic = magicProxyTCP
	case ProxyStreamUDP:
		magic = magicProxyUDP
	default:
		return fmt.Errorf("tunnel: unknown proxy stream kind %q", kind)
	}
	if _, err := io.WriteString(w, magic); err != nil {
		return err
	}
	if _, err := w.Write([]byte{protocolV1}); err != nil {
		return err
	}
	_, err := mcproto.WriteString(w, service)
	return err
}

func readProxyStreamHeader(r io.Reader) (kind ProxyStreamKind, service string, err error) {
	var hdr [4]byte
	if _, err := io.ReadFull(r, hdr[:]); err != nil {
		return "", "", err
	}
	switch string(hdr[:]) {
	case magicProxyTCP:
		kind = ProxyStreamTCP
	case magicProxyUDP:
		kind = ProxyStreamUDP
	default:
		return "", "", ErrBadMagic
	}
	var ver [1]byte
	if _, err := io.ReadFull(r, ver[:]); err != nil {
		return "", "", err
	}
	if ver[0] != protocolV1 {
		return "", "", ErrBadVersion
	}
	s, _, err := mcproto.ReadString(r)
	if err != nil {
		return "", "", err
	}
	s = strings.TrimSpace(s)
	if s == "" {
		return "", "", fmt.Errorf("tunnel: empty service")
	}
	return kind, s, nil
}
