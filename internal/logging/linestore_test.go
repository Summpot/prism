package logging

import "testing"

func TestLineStore_SnapshotRing(t *testing.T) {
	ls := NewLineStore(3)
	if _, err := ls.Write([]byte("a\nb\nc\n")); err != nil {
		t.Fatalf("Write: %v", err)
	}
	got := ls.Snapshot(0)
	want := []string{"a", "b", "c"}
	if len(got) != len(want) {
		t.Fatalf("snapshot len=%d want=%d: %#v", len(got), len(want), got)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("snapshot[%d]=%q want %q", i, got[i], want[i])
		}
	}

	_, _ = ls.Write([]byte("d\n"))
	got = ls.Snapshot(0)
	want = []string{"b", "c", "d"}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("after overwrite snapshot[%d]=%q want %q", i, got[i], want[i])
		}
	}
	if ls.Dropped() != 1 {
		t.Fatalf("dropped=%d want=1", ls.Dropped())
	}
}

func TestLineStore_PartialLines(t *testing.T) {
	ls := NewLineStore(10)
	_, _ = ls.Write([]byte("hello"))
	if got := ls.Snapshot(0); len(got) != 0 {
		t.Fatalf("expected no complete lines yet, got %#v", got)
	}
	_, _ = ls.Write([]byte(" world\n"))
	got := ls.Snapshot(0)
	if len(got) != 1 || got[0] != "hello world" {
		t.Fatalf("snapshot=%#v want [hello world]", got)
	}
}

func TestLineStore_Limit(t *testing.T) {
	ls := NewLineStore(10)
	_, _ = ls.Write([]byte("a\nb\nc\nd\n"))
	got := ls.Snapshot(2)
	want := []string{"c", "d"}
	if len(got) != len(want) {
		t.Fatalf("snapshot len=%d want=%d: %#v", len(got), len(want), got)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("snapshot[%d]=%q want %q", i, got[i], want[i])
		}
	}
}
