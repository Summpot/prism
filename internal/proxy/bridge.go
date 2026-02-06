package proxy

import (
	"context"
	"errors"
	"io"
	"net"
	"sync"
)

type BridgeMetrics interface {
	AddIngress(n int64)
	AddEgress(n int64)
}

type ProxyBridgeOptions struct {
	BufferPool         BufferPool
	InjectProxyProtoV2 bool
	Metrics            BridgeMetrics
}

type ProxyBridge struct {
	opts ProxyBridgeOptions
}

func NewProxyBridge(opts ProxyBridgeOptions) *ProxyBridge {
	return &ProxyBridge{opts: opts}
}

func (b *ProxyBridge) buffer() []byte {
	if b.opts.BufferPool != nil {
		return b.opts.BufferPool.Get()
	}
	return make([]byte, 32*1024)
}

func (b *ProxyBridge) putBuffer(buf []byte) {
	if b.opts.BufferPool != nil {
		b.opts.BufferPool.Put(buf)
	}
}

func (b *ProxyBridge) Proxy(ctx context.Context, client net.Conn, upstream net.Conn, initialClientToUpstream io.Reader) error {
	// Close both sides on return; io.Copy goroutines will exit.
	defer client.Close()
	defer upstream.Close()

	if b.opts.InjectProxyProtoV2 {
		src, _ := client.RemoteAddr().(*net.TCPAddr)
		dst, _ := upstream.RemoteAddr().(*net.TCPAddr)
		if src != nil && dst != nil {
			if hdr, err := BuildProxyV2Header(src, dst); err == nil {
				if _, err := upstream.Write(hdr); err != nil {
					return err
				}
			}
		}
	}

	errCh := make(chan error, 2)
	var wg sync.WaitGroup
	copyFn := func(dst net.Conn, src io.Reader, countFn func(int64)) {
		defer wg.Done()
		buf := b.buffer()
		defer b.putBuffer(buf)
		written, err := io.CopyBuffer(dst, src, buf)
		if written > 0 && countFn != nil {
			countFn(written)
		}
		// Ignore net.ErrClosed to reduce noise during shutdown races.
		if err != nil && !errors.Is(err, net.ErrClosed) {
			errCh <- err
			return
		}
		errCh <- nil
	}

	var ingressFn func(int64)
	var egressFn func(int64)
	if b.opts.Metrics != nil {
		ingressFn = b.opts.Metrics.AddIngress
		egressFn = b.opts.Metrics.AddEgress
	}

	// client -> upstream (includes bytes already read for handshake)
	wg.Add(1)
	go copyFn(upstream, initialClientToUpstream, ingressFn)

	// upstream -> client
	wg.Add(1)
	go copyFn(client, upstream, egressFn)

	select {
	case <-ctx.Done():
		_ = client.Close()
		_ = upstream.Close()
		wg.Wait()
		return ctx.Err()
	case err := <-errCh:
		_ = client.Close()
		_ = upstream.Close()
		wg.Wait()
		// Drain the other side.
		<-errCh
		return err
	}
}
