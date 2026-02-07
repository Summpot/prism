package tunnel

import (
	"bufio"
	"encoding/binary"
	"fmt"
	"io"
	"net"
	"time"
)

// DatagramConn wraps a stream net.Conn and provides datagram-style semantics
// using a simple length-prefixed framing.
//
// Each Write sends exactly one datagram.
// Each Read returns exactly one datagram.
//
// This is used to proxy UDP traffic over a multiplexed tunnel stream.
//
//nolint:revive // name is intentional.
type DatagramConn struct {
	st net.Conn
	r  *bufio.Reader
}

func NewDatagramConn(st net.Conn) *DatagramConn {
	return &DatagramConn{st: st, r: bufio.NewReader(st)}
}

func (c *DatagramConn) Read(p []byte) (int, error) {
	var lenBuf [4]byte
	if _, err := io.ReadFull(c.r, lenBuf[:]); err != nil {
		return 0, err
	}
	n := binary.BigEndian.Uint32(lenBuf[:])
	if n > 1<<20 { // 1 MiB cap
		return 0, fmt.Errorf("tunnel: datagram too large: %d", n)
	}
	if int(n) > len(p) {
		// Drain the frame to keep the stream aligned.
		buf := make([]byte, int(n))
		if _, err := io.ReadFull(c.r, buf); err != nil {
			return 0, err
		}
		return 0, io.ErrShortBuffer
	}
	_, err := io.ReadFull(c.r, p[:n])
	if err != nil {
		return 0, err
	}
	return int(n), nil
}

func (c *DatagramConn) Write(p []byte) (int, error) {
	var lenBuf [4]byte
	binary.BigEndian.PutUint32(lenBuf[:], uint32(len(p)))
	if _, err := c.st.Write(lenBuf[:]); err != nil {
		return 0, err
	}
	n, err := c.st.Write(p)
	if err != nil {
		return n, err
	}
	// Write reports payload length.
	return len(p), nil
}

func (c *DatagramConn) Close() error { return c.st.Close() }
func (c *DatagramConn) LocalAddr() net.Addr {
	return c.st.LocalAddr()
}
func (c *DatagramConn) RemoteAddr() net.Addr {
	return c.st.RemoteAddr()
}
func (c *DatagramConn) SetDeadline(t time.Time) error {
	return c.st.SetDeadline(t)
}
func (c *DatagramConn) SetReadDeadline(t time.Time) error {
	return c.st.SetReadDeadline(t)
}
func (c *DatagramConn) SetWriteDeadline(t time.Time) error {
	return c.st.SetWriteDeadline(t)
}

var _ net.Conn = (*DatagramConn)(nil)
