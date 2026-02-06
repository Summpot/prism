package logging

import (
	"bytes"
	"sync"
	"sync/atomic"
)

// LineStore keeps the last N log lines in memory.
//
// It is intended for debugging via the admin server (e.g. GET /logs) and is not
// a replacement for durable log shipping.
//
// Concurrency: safe for concurrent use.
// Memory: bounded by the configured line capacity.
type LineStore struct {
	size int

	mu    sync.Mutex
	lines []string
	next  int
	count int
	buf   []byte // partial line buffer

	dropped atomic.Uint64
}

func NewLineStore(size int) *LineStore {
	if size < 0 {
		size = 0
	}
	ls := &LineStore{size: size}
	if size > 0 {
		ls.lines = make([]string, size)
	}
	return ls
}

// Write implements io.Writer and stores complete lines delimited by '\n'.
// Carriage returns ('\r') at the end of a line are trimmed.
func (s *LineStore) Write(p []byte) (int, error) {
	if s == nil || s.size == 0 {
		return len(p), nil
	}
	origN := len(p)

	s.mu.Lock()
	defer s.mu.Unlock()

	for len(p) > 0 {
		i := bytes.IndexByte(p, '\n')
		if i < 0 {
			// No newline: buffer and return.
			s.buf = append(s.buf, p...)
			break
		}

		chunk := p[:i]
		p = p[i+1:]

		line := append(s.buf, chunk...)
		s.buf = s.buf[:0]
		if n := len(line); n > 0 && line[n-1] == '\r' {
			line = line[:n-1]
		}
		s.addLineLocked(string(line))
	}

	return origN, nil
}

func (s *LineStore) addLineLocked(line string) {
	if s.count < s.size {
		s.lines[s.next] = line
		s.next = (s.next + 1) % s.size
		s.count++
		return
	}

	// Overwrite oldest.
	s.lines[s.next] = line
	s.next = (s.next + 1) % s.size
	s.dropped.Add(1)
}

// Snapshot returns up to limit most recent lines, ordered oldest->newest.
// If limit <= 0, all buffered lines are returned.
func (s *LineStore) Snapshot(limit int) []string {
	if s == nil {
		return nil
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	if s.count == 0 {
		return nil
	}

	n := s.count
	if limit > 0 && limit < n {
		n = limit
	}

	start := s.count - n
	out := make([]string, 0, n)
	for i := start; i < s.count; i++ {
		// Oldest index is next-count.
		idx := s.next - s.count + i
		for idx < 0 {
			idx += s.size
		}
		idx = idx % s.size
		out = append(out, s.lines[idx])
	}
	return out
}

func (s *LineStore) Dropped() uint64 {
	if s == nil {
		return 0
	}
	return s.dropped.Load()
}
