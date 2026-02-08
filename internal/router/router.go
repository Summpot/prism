package router

import (
	"fmt"
	"math/rand"
	"regexp"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

// UpstreamResolver resolves a host to an ordered list of candidate upstreams.
// The caller should try Upstreams in order until a dial succeeds.
type UpstreamResolver interface {
	Resolve(host string) (res Resolution, ok bool)
}

// Route is the input configuration for a single route.
// Matching is performed in the order routes are provided.
type Route struct {
	Host         []string
	Upstreams    []string
	Strategy     string
	CachePingTTL time.Duration
}

type Resolution struct {
	Upstreams    []string
	CachePingTTL time.Duration
	MatchedHost  string
}

type compiledRoutes struct {
	routes []compiledRoute
}

type compiledRoute struct {
	patterns []compiledPattern

	upstreams []string
	strategy  strategy

	cachePingTTL time.Duration

	rr atomic.Uint64
}

type compiledPattern struct {
	pattern string
	exact   bool
	re      *regexp.Regexp
}

type strategy int

const (
	strategySequential strategy = iota
	strategyRandom
	strategyRoundRobin
)

// Router resolves a hostname to a set of upstream addresses.
// Reads are lock-free via atomic snapshots; updates swap the snapshot.
type Router struct {
	v atomic.Value // *compiledRoutes
}

func NewRouter(routes []Route) *Router {
	r := &Router{}
	r.Update(routes)
	return r
}

func (r *Router) Update(routes []Route) {
	cr := &compiledRoutes{}
	if len(routes) == 0 {
		r.v.Store(cr)
		return
	}

	compiled := make([]compiledRoute, 0, len(routes))
	for i := range routes {
		rt := routes[i]
		c, err := compileRoute(rt)
		if err != nil {
			// Invalid routes are skipped rather than making routing unusable.
			// Config parsing should catch these in normal operation.
			continue
		}
		compiled = append(compiled, c)
	}
	cr.routes = compiled
	r.v.Store(cr)
}

func compileRoute(rt Route) (compiledRoute, error) {
	pat := make([]compiledPattern, 0, len(rt.Host))
	for _, h := range rt.Host {
		h = strings.TrimSpace(strings.ToLower(h))
		if h == "" {
			continue
		}
		cp := compiledPattern{pattern: h}
		if !strings.ContainsAny(h, "*?") {
			cp.exact = true
			pat = append(pat, cp)
			continue
		}
		re, err := compileWildcardPattern(h)
		if err != nil {
			return compiledRoute{}, fmt.Errorf("router: compile host pattern %q: %w", h, err)
		}
		cp.re = re
		pat = append(pat, cp)
	}
	if len(pat) == 0 {
		return compiledRoute{}, fmt.Errorf("router: route missing host patterns")
	}

	up := make([]string, 0, len(rt.Upstreams))
	for _, u := range rt.Upstreams {
		u = strings.TrimSpace(u)
		if u == "" {
			continue
		}
		up = append(up, u)
	}
	if len(up) == 0 {
		return compiledRoute{}, fmt.Errorf("router: route missing upstreams")
	}

	st := parseStrategy(rt.Strategy)
	cacheTTL := rt.CachePingTTL

	return compiledRoute{patterns: pat, upstreams: up, strategy: st, cachePingTTL: cacheTTL}, nil
}

func parseStrategy(s string) strategy {
	s = strings.TrimSpace(strings.ToLower(s))
	s = strings.ReplaceAll(s, "_", "-")
	s = strings.ReplaceAll(s, " ", "-")
	s = strings.ReplaceAll(s, "--", "-")
	if s == "" {
		return strategySequential
	}
	switch s {
	case "sequential":
		return strategySequential
	case "random":
		return strategyRandom
	case "round-robin", "roundrobin":
		return strategyRoundRobin
	default:
		// Default to sequential for unknown values.
		return strategySequential
	}
}

// compileWildcardPattern compiles a wildcard host pattern into a regexp.
//
// Supported wildcards:
//   - '*' matches any sequence (including empty) and captures it
//   - '?' matches any single character and captures it
func compileWildcardPattern(pattern string) (*regexp.Regexp, error) {
	pattern = strings.TrimSpace(strings.ToLower(pattern))
	if pattern == "" {
		return nil, fmt.Errorf("empty pattern")
	}

	var b strings.Builder
	b.Grow(len(pattern) + 16)
	b.WriteByte('^')

	escapeNext := false
	for _, r := range pattern {
		if escapeNext {
			b.WriteRune(r)
			escapeNext = false
			continue
		}
		switch r {
		case '*':
			b.WriteString("(.*?)")
		case '?':
			b.WriteString("(.)")
		case '\\':
			escapeNext = true
			b.WriteString("\\")
		default:
			// Escape regex special characters.
			if strings.ContainsRune(".^$+()[]{}|", r) {
				b.WriteByte('\\')
			}
			b.WriteRune(r)
		}
	}

	b.WriteByte('$')
	return regexp.Compile(b.String())
}

var (
	rngMu sync.Mutex
	rng   = rand.New(rand.NewSource(time.Now().UnixNano()))
)

func (r *Router) Resolve(host string) (Resolution, bool) {
	cr, _ := r.v.Load().(*compiledRoutes)
	if cr == nil || len(cr.routes) == 0 {
		return Resolution{}, false
	}

	host = strings.TrimSpace(strings.ToLower(host))
	if host == "" {
		return Resolution{}, false
	}

	for i := range cr.routes {
		rt := &cr.routes[i]
		for _, p := range rt.patterns {
			matched, groups := matchHost(host, p)
			if !matched {
				continue
			}

			// Apply parameter substitution ($1, $2, ...) on upstream templates.
			candidates := make([]string, 0, len(rt.upstreams))
			for _, u := range rt.upstreams {
				candidates = append(candidates, substituteParams(u, groups))
			}
			candidates = orderCandidates(rt, candidates)

			return Resolution{Upstreams: candidates, CachePingTTL: rt.cachePingTTL, MatchedHost: p.pattern}, true
		}
	}

	return Resolution{}, false
}

func matchHost(host string, p compiledPattern) (bool, []string) {
	if p.exact {
		return host == p.pattern, nil
	}
	if p.re == nil {
		return false, nil
	}
	m := p.re.FindStringSubmatch(host)
	if m == nil {
		return false, nil
	}
	if len(m) <= 1 {
		return true, nil
	}
	return true, m[1:]
}

func substituteParams(template string, groups []string) string {
	if len(groups) == 0 || template == "" {
		return template
	}
	res := template
	for i := len(groups); i >= 1; i-- {
		param := fmt.Sprintf("$%d", i)
		res = strings.ReplaceAll(res, param, groups[i-1])
	}
	return res
}

func orderCandidates(rt *compiledRoute, candidates []string) []string {
	if len(candidates) <= 1 {
		return candidates
	}

	switch rt.strategy {
	case strategySequential:
		return candidates
	case strategyRandom:
		rngMu.Lock()
		start := rng.Intn(len(candidates))
		rngMu.Unlock()
		return rotate(candidates, start)
	case strategyRoundRobin:
		start := int(rt.rr.Add(1)-1) % len(candidates)
		return rotate(candidates, start)
	default:
		return candidates
	}
}

func rotate(in []string, start int) []string {
	n := len(in)
	if n == 0 {
		return in
	}
	start = start % n
	if start == 0 {
		return in
	}
	out := make([]string, 0, n)
	out = append(out, in[start:]...)
	out = append(out, in[:start]...)
	return out
}

var _ UpstreamResolver = (*Router)(nil)
