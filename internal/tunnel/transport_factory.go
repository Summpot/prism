package tunnel

import "fmt"

func TransportByName(name string) (Transport, error) {
	n, err := ParseTransport(name)
	if err != nil {
		return nil, err
	}
	switch n {
	case "tcp":
		return NewTCPTransport(), nil
	case "udp":
		return NewUDPTransport(), nil
	case "quic":
		return NewQUICTransport(), nil
	default:
		return nil, fmt.Errorf("tunnel: transport not implemented: %s", n)
	}
}
