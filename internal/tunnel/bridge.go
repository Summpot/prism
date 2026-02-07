package tunnel

import (
	"context"
	"errors"
	"io"
	"net"
	"sync"
)

type Bridge struct {
	pool sync.Pool
}

func NewBridge(bufSize int) *Bridge {
	if bufSize <= 0 {
		bufSize = 32 * 1024
	}
	b := &Bridge{}
	b.pool.New = func() any {
		buf := make([]byte, bufSize)
		return &buf
	}
	return b
}

func (b *Bridge) get() []byte {
	p := b.pool.Get().(*[]byte)
	return (*p)[:]
}

func (b *Bridge) put(buf []byte) {
	b.pool.Put(&buf)
}

func (b *Bridge) Proxy(ctx context.Context, a net.Conn, bconn net.Conn) error {
	defer a.Close()
	defer bconn.Close()

	errCh := make(chan error, 2)
	var wg sync.WaitGroup

	copyFn := func(dst net.Conn, src net.Conn) {
		defer wg.Done()
		buf := b.get()
		defer b.put(buf)
		_, err := io.CopyBuffer(dst, src, buf)
		if err != nil && !errors.Is(err, net.ErrClosed) {
			errCh <- err
			return
		}
		errCh <- nil
	}

	wg.Add(2)
	go copyFn(a, bconn)
	go copyFn(bconn, a)

	select {
	case <-ctx.Done():
		_ = a.Close()
		_ = bconn.Close()
		wg.Wait()
		return ctx.Err()
	case err := <-errCh:
		_ = a.Close()
		_ = bconn.Close()
		wg.Wait()
		<-errCh
		return err
	}
}
