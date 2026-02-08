package router

import "testing"

func TestRouterExactAndWildcard(t *testing.T) {
	r := NewRouter([]Route{
		{Host: []string{"play.example.com"}, Upstreams: []string{"10.0.0.1:25565"}},
		{Host: []string{"*.labs.example.com"}, Upstreams: []string{"10.0.0.2:25565"}},
		{Host: []string{"*.example.com"}, Upstreams: []string{"10.0.0.3:25565"}},
	})

	if res, ok := r.Resolve("play.example.com"); !ok || res.Upstreams[0] != "10.0.0.1:25565" {
		t.Fatalf("exact resolve failed: %v %v", ok, res.Upstreams)
	}
	if res, ok := r.Resolve("a.labs.example.com"); !ok || res.Upstreams[0] != "10.0.0.2:25565" {
		t.Fatalf("wildcard resolve failed: %v %v", ok, res.Upstreams)
	}
	if res, ok := r.Resolve("b.example.com"); !ok || res.Upstreams[0] != "10.0.0.3:25565" {
		t.Fatalf("fallback wildcard resolve failed: %v %v", ok, res.Upstreams)
	}
	if _, ok := r.Resolve("example.com"); ok {
		t.Fatalf("wildcard should not match root domain")
	}
}

func TestRouter_ParamSubstitution(t *testing.T) {
	r := NewRouter([]Route{{
		Host:      []string{"*.domain.com"},
		Upstreams: []string{"$1.servers.svc:25565"},
	}})

	res, ok := r.Resolve("abc.domain.com")
	if !ok {
		t.Fatalf("expected match")
	}
	if got := res.Upstreams[0]; got != "abc.servers.svc:25565" {
		t.Fatalf("upstream substitution: got %q", got)
	}
}

func TestRouter_RoundRobin(t *testing.T) {
	r := NewRouter([]Route{{
		Host:      []string{"play.example.com"},
		Upstreams: []string{"a:1", "b:1", "c:1"},
		Strategy:  "round-robin",
	}})

	first := make([]string, 0, 6)
	for i := 0; i < 6; i++ {
		res, ok := r.Resolve("play.example.com")
		if !ok {
			t.Fatalf("expected match")
		}
		first = append(first, res.Upstreams[0])
	}

	// Should cycle through candidates.
	seen := map[string]bool{}
	for _, v := range first {
		seen[v] = true
	}
	if len(seen) != 3 {
		t.Fatalf("expected rr to select all 3 upstreams, got %v", first)
	}
}
