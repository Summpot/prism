package mcproto

import (
	"bytes"
	"testing"
)

func TestVarIntRoundTrip(t *testing.T) {
	vals := []int32{0, 1, 2, 127, 128, 255, 2147483647, -1, -2147483648}
	for _, v := range vals {
		var buf bytes.Buffer
		_, err := WriteVarInt(&buf, v)
		if err != nil {
			t.Fatalf("WriteVarInt(%d): %v", v, err)
		}
		got, _, err := ReadVarInt(&buf)
		if err != nil {
			t.Fatalf("ReadVarInt(%d): %v", v, err)
		}
		if got != v {
			t.Fatalf("roundtrip: want %d got %d", v, got)
		}
	}
}
