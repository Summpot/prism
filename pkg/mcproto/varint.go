package mcproto

import (
	"encoding/binary"
	"errors"
	"fmt"
	"io"
)

var (
	ErrVarIntTooLong = errors.New("mcproto: varint too long")
	ErrVarIntEOF     = errors.New("mcproto: unexpected EOF")
)

// ReadVarInt reads a Minecraft-style VarInt (signed 32-bit, little-endian 7-bit groups).
// It returns the decoded value and the number of bytes consumed.
func ReadVarInt(r io.Reader) (int32, int, error) {
	var (
		numRead int
		result  int32
	)

	for {
		if numRead >= 5 {
			return 0, numRead, ErrVarIntTooLong
		}

		b, err := readOneByte(r)
		if err != nil {
			if errors.Is(err, io.EOF) {
				return 0, numRead, ErrVarIntEOF
			}
			return 0, numRead, err
		}

		value := int32(b & 0x7F)
		result |= value << (7 * numRead)
		numRead++

		if (b & 0x80) == 0 {
			return result, numRead, nil
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

func WriteVarInt(w io.Writer, v int32) (int, error) {
	var out [5]byte
	ux := uint32(v)
	i := 0
	for {
		b := byte(ux & 0x7F)
		ux >>= 7
		if ux != 0 {
			b |= 0x80
		}
		out[i] = b
		i++
		if ux == 0 {
			break
		}
		if i >= len(out) {
			return 0, ErrVarIntTooLong
		}
	}

	n, err := w.Write(out[:i])
	return n, err
}

func ReadString(r io.Reader) (string, int, error) {
	ln, n1, err := ReadVarInt(r)
	if err != nil {
		return "", n1, err
	}
	if ln < 0 {
		return "", n1, fmt.Errorf("mcproto: negative string length: %d", ln)
	}
	buf := make([]byte, int(ln))
	n2, err := io.ReadFull(r, buf)
	if err != nil {
		return "", n1 + n2, err
	}
	return string(buf), n1 + n2, nil
}

func WriteString(w io.Writer, s string) (int, error) {
	n1, err := WriteVarInt(w, int32(len(s)))
	if err != nil {
		return n1, err
	}
	n2, err := io.WriteString(w, s)
	return n1 + n2, err
}

func ReadUShort(r io.Reader) (uint16, int, error) {
	var buf [2]byte
	n, err := io.ReadFull(r, buf[:])
	if err != nil {
		return 0, n, err
	}
	return binary.BigEndian.Uint16(buf[:]), n, nil
}

func WriteUShort(w io.Writer, v uint16) (int, error) {
	var buf [2]byte
	binary.BigEndian.PutUint16(buf[:], v)
	return w.Write(buf[:])
}
