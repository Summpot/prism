package proxy

import (
	"context"
	"fmt"
	"sync"
	"time"

	"golang.org/x/sync/singleflight"
)

type StatusCacheKey struct {
	Upstream        string
	ProtocolVersion int32
}

type statusCacheItem struct {
	expiresAt time.Time
	data      []byte
}

// StatusCache caches raw Minecraft Status response packets (length-prefixed frames).
//
// Entries are stored per-upstream and protocol version with a per-route TTL.
// Failed loads are not cached.
//
// This cache is optimized for correctness and simplicity; it performs lazy
// expiration (no background janitor).
type StatusCache struct {
	mu    sync.Mutex
	items map[StatusCacheKey]statusCacheItem
	sf    singleflight.Group
}

func NewStatusCache() *StatusCache {
	return &StatusCache{items: make(map[StatusCacheKey]statusCacheItem)}
}

var (
	defaultStatusCacheOnce sync.Once
	defaultStatusCacheInst *StatusCache
)

func DefaultStatusCache() *StatusCache {
	defaultStatusCacheOnce.Do(func() {
		defaultStatusCacheInst = NewStatusCache()
	})
	return defaultStatusCacheInst
}

func (c *StatusCache) Get(key StatusCacheKey) ([]byte, bool) {
	if c == nil {
		return nil, false
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	it, ok := c.items[key]
	if !ok {
		return nil, false
	}
	if !it.expiresAt.IsZero() && time.Now().After(it.expiresAt) {
		delete(c.items, key)
		return nil, false
	}
	if len(it.data) == 0 {
		return nil, false
	}
	out := make([]byte, len(it.data))
	copy(out, it.data)
	return out, true
}

func (c *StatusCache) Set(key StatusCacheKey, data []byte, ttl time.Duration) {
	if c == nil {
		return
	}
	if ttl <= 0 {
		return
	}
	if len(data) == 0 {
		return
	}
	copyData := make([]byte, len(data))
	copy(copyData, data)

	exp := time.Now().Add(ttl)
	c.mu.Lock()
	c.items[key] = statusCacheItem{expiresAt: exp, data: copyData}
	c.mu.Unlock()
}

func (c *StatusCache) GetOrLoad(ctx context.Context, key StatusCacheKey, ttl time.Duration, load func(context.Context) ([]byte, error)) ([]byte, error) {
	if ttl <= 0 {
		return load(ctx)
	}
	if data, ok := c.Get(key); ok {
		return data, nil
	}
	if c == nil {
		c = DefaultStatusCache()
	}

	sfKey := fmt.Sprintf("%s\x00%d", key.Upstream, key.ProtocolVersion)
	v, err, _ := c.sf.Do(sfKey, func() (any, error) {
		if data, ok := c.Get(key); ok {
			return data, nil
		}
		data, err := load(ctx)
		if err != nil {
			return nil, err
		}
		c.Set(key, data, ttl)
		// Return a copy to keep callers isolated.
		out := make([]byte, len(data))
		copy(out, data)
		return out, nil
	})
	if err != nil {
		return nil, err
	}
	b, _ := v.([]byte)
	return b, nil
}
