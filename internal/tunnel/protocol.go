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
	magicProxy    = "PRPX" // Prism Reverse Proxy
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
	Name      string `json:"name"`
	LocalAddr string `json:"local_addr"` // only used by prismc; prisms stores it for debugging
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
	}
	return req, nil
}

func writeProxyStreamHeader(w io.Writer, service string) error {
	if _, err := io.WriteString(w, magicProxy); err != nil {
		return err
	}
	if _, err := w.Write([]byte{protocolV1}); err != nil {
		return err
	}
	_, err := mcproto.WriteString(w, service)
	return err
}

func readProxyStreamHeader(r io.Reader) (service string, err error) {
	r = bufio.NewReader(r)
	var hdr [4]byte
	if _, err := io.ReadFull(r, hdr[:]); err != nil {
		return "", err
	}
	if string(hdr[:]) != magicProxy {
		return "", ErrBadMagic
	}
	var ver [1]byte
	if _, err := io.ReadFull(r, ver[:]); err != nil {
		return "", err
	}
	if ver[0] != protocolV1 {
		return "", ErrBadVersion
	}
	s, _, err := mcproto.ReadString(r)
	if err != nil {
		return "", err
	}
	s = strings.TrimSpace(s)
	if s == "" {
		return "", fmt.Errorf("tunnel: empty service")
	}
	return s, nil
}
