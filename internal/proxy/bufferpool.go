package proxy

import "sync"

type BufferPool interface {
	Get() []byte
	Put([]byte)
}

type SyncPoolBufferPool struct {
	size int
	p    sync.Pool
}

func NewSyncPoolBufferPool(size int) *SyncPoolBufferPool {
	bp := &SyncPoolBufferPool{size: size}
	bp.p.New = func() any { return make([]byte, bp.size) }
	return bp
}

func (p *SyncPoolBufferPool) Get() []byte {
	return p.p.Get().([]byte)
}

func (p *SyncPoolBufferPool) Put(b []byte) {
	if cap(b) < p.size {
		return
	}
	// Normalize len so callers don't accidentally keep huge slices alive.
	b = b[:p.size]
	p.p.Put(b)
}
