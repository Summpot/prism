package protocol

import (
	"errors"
)

var (
	ErrNeedMoreData = errors.New("protocol: need more data")
	ErrNoMatch      = errors.New("protocol: no match")
)

// HostParser extracts a routing hostname from the captured initial bytes of a connection.
//
// Parse should return:
//  - (host, nil) when it successfully extracted a hostname
//  - ("", ErrNeedMoreData) when more bytes are required
//  - ("", ErrNoMatch) when the parser does not apply to this stream
//  - ("", err) for fatal errors
//
// Implementations must be pure with respect to input bytes.
// They may be called multiple times with increasing prefixes of the same stream.
type HostParser interface {
	Name() string
	Parse(prelude []byte) (string, error)
}

type ChainHostParser struct {
	parsers []HostParser
}

func NewChainHostParser(parsers ...HostParser) *ChainHostParser {
	out := make([]HostParser, 0, len(parsers))
	for _, p := range parsers {
		if p != nil {
			out = append(out, p)
		}
	}
	return &ChainHostParser{parsers: out}
}

func (p *ChainHostParser) Name() string { return "chain" }

func (p *ChainHostParser) Parse(prelude []byte) (string, error) {
	var needMore bool
	for _, sp := range p.parsers {
		host, err := sp.Parse(prelude)
		if err == nil {
			if host == "" {
				// Treat empty host as a non-match to keep callers simple.
				continue
			}
			return host, nil
		}
		if errors.Is(err, ErrNeedMoreData) {
			needMore = true
			continue
		}
		if errors.Is(err, ErrNoMatch) {
			continue
		}
		return "", err
	}
	if needMore {
		return "", ErrNeedMoreData
	}
	return "", ErrNoMatch
}

var _ HostParser = (*ChainHostParser)(nil)
