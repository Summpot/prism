package router

import "testing"

func TestRouterExactAndWildcard(t *testing.T) {
	r := NewRouter(map[string]string{
		"play.example.com":    "10.0.0.1:25565",
		"*.labs.example.com":  "10.0.0.2:25565",
		"*.example.com":       "10.0.0.3:25565",
	})

	if up, ok := r.Resolve("play.example.com"); !ok || up != "10.0.0.1:25565" {
		t.Fatalf("exact resolve failed: %v %v", ok, up)
	}
	if up, ok := r.Resolve("a.labs.example.com"); !ok || up != "10.0.0.2:25565" {
		t.Fatalf("wildcard resolve failed: %v %v", ok, up)
	}
	if up, ok := r.Resolve("b.example.com"); !ok || up != "10.0.0.3:25565" {
		t.Fatalf("fallback wildcard resolve failed: %v %v", ok, up)
	}
	if _, ok := r.Resolve("example.com"); ok {
		t.Fatalf("wildcard should not match root domain")
	}
}
