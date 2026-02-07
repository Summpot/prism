package tunnel

import (
	"bytes"
	"testing"
)

func TestRegisterRequest_RouteOnlyClearsRemoteAddr(t *testing.T) {
	var buf bytes.Buffer
	if err := writeRegisterRequest(&buf, RegisterRequest{Services: []RegisteredService{{
		Name:       " svc ",
		LocalAddr:  " 127.0.0.1:25565 ",
		RouteOnly:  true,
		RemoteAddr: ":25565",
	}}}); err != nil {
		t.Fatalf("writeRegisterRequest: %v", err)
	}

	req, err := readRegisterRequest(&buf)
	if err != nil {
		t.Fatalf("readRegisterRequest: %v", err)
	}
	if len(req.Services) != 1 {
		t.Fatalf("Services len=%d want 1", len(req.Services))
	}
	s := req.Services[0]
	if s.Name != "svc" {
		t.Fatalf("Name=%q want %q", s.Name, "svc")
	}
	if s.Proto != "tcp" {
		t.Fatalf("Proto=%q want %q", s.Proto, "tcp")
	}
	if s.LocalAddr != "127.0.0.1:25565" {
		t.Fatalf("LocalAddr=%q want trimmed", s.LocalAddr)
	}
	if !s.RouteOnly {
		t.Fatalf("RouteOnly=false want true")
	}
	if s.RemoteAddr != "" {
		t.Fatalf("RemoteAddr=%q want empty when route_only=true", s.RemoteAddr)
	}
}
