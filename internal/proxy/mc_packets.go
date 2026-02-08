package proxy

import (
	"bytes"
	"errors"
	"fmt"
	"io"

	"prism/pkg/mcproto"
)

func readPacketRaw(r io.Reader, maxPacketLen int) (raw []byte, packetID int32, err error) {
	if maxPacketLen <= 0 {
		maxPacketLen = 512 * 1024
	}

	ln, lnRaw, err := readVarIntRaw(r)
	if err != nil {
		return nil, 0, err
	}
	if ln < 0 {
		return nil, 0, fmt.Errorf("protocol: negative packet length")
	}
	if int(ln) > maxPacketLen {
		return nil, 0, fmt.Errorf("protocol: packet too large: %d", ln)
	}

	payload := make([]byte, int(ln))
	if _, err := io.ReadFull(r, payload); err != nil {
		return nil, 0, err
	}

	br := bytes.NewReader(payload)
	pid, _, err := mcproto.ReadVarInt(br)
	if err != nil {
		return nil, 0, err
	}

	out := make([]byte, 0, len(lnRaw)+len(payload))
	out = append(out, lnRaw...)
	out = append(out, payload...)
	return out, pid, nil
}

func readVarIntRaw(r io.Reader) (value int32, raw []byte, err error) {
	var (
		numRead int
		result  int32
		buf     [5]byte
	)

	for {
		if numRead >= 5 {
			return 0, buf[:numRead], mcproto.ErrVarIntTooLong
		}
		b, rerr := readOneByte(r)
		if rerr != nil {
			// If we haven't read anything yet, surface EOF cleanly.
			if errors.Is(rerr, io.EOF) && numRead == 0 {
				return 0, nil, io.EOF
			}
			return 0, buf[:numRead], rerr
		}
		buf[numRead] = b
		valueBits := int32(b & 0x7F)
		result |= valueBits << (7 * numRead)
		numRead++
		if (b & 0x80) == 0 {
			return result, buf[:numRead], nil
		}
	}
}

func readOneByte(r io.Reader) (byte, error) {
	if br, ok := r.(interface{ ReadByte() (byte, error) }); ok {
		return br.ReadByte()
	}
	var one [1]byte
	_, err := io.ReadFull(r, one[:])
	return one[0], err
}
