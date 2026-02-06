package router

import (
	"sort"
	"strings"
	"sync/atomic"
)

type UpstreamResolver interface {
	Resolve(host string) (upstreamAddr string, ok bool)
}

type compiledRoutes struct {
	exact    map[string]string
	wildcard []wildRoute
}

type wildRoute struct {
	suffix   string
	upstream string
}

// Router resolves a hostname to an upstream address.
// Reads are lock-free via atomic snapshots; updates swap the snapshot.
type Router struct {
	v atomic.Value // *compiledRoutes
}

func NewRouter(routes map[string]string) *Router {
	r := &Router{}
	r.Update(routes)
	return r
}

func (r *Router) Update(routes map[string]string) {
	cr := &compiledRoutes{exact: map[string]string{}}
	for k, v := range routes {
		k = strings.TrimSpace(strings.ToLower(k))
		if k == "" {
			continue
		}
		if strings.HasPrefix(k, "*.") && len(k) > 2 {
			cr.wildcard = append(cr.wildcard, wildRoute{suffix: strings.TrimPrefix(k, "*."), upstream: v})
			continue
		}
		cr.exact[k] = v
	}

	// Prefer more specific wildcards: longer suffix first.
	sort.Slice(cr.wildcard, func(i, j int) bool {
		return len(cr.wildcard[i].suffix) > len(cr.wildcard[j].suffix)
	})

	r.v.Store(cr)
}

func (r *Router) Resolve(host string) (string, bool) {
	cr, _ := r.v.Load().(*compiledRoutes)
	if cr == nil {
		return "", false
	}

	host = strings.TrimSpace(strings.ToLower(host))
	if host == "" {
		return "", false
	}

	if up, ok := cr.exact[host]; ok {
		return up, true
	}

	for _, wr := range cr.wildcard {
		if host == wr.suffix {
			continue
		}
		if strings.HasSuffix(host, "."+wr.suffix) {
			return wr.upstream, true
		}
	}

	return "", false
}
