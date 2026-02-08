package protocol

import (
	"context"
	"embed"
	"fmt"
	"strings"
	"sync"
)

// Builtin WASM routing parsers are shipped as precompiled WASM binaries embedded in the Prism binary.
// They can be referenced from config with `type="wasm"` and `path="builtin:<name>"`.
//
// This keeps the routing parser ABI stable while allowing the default parsers
// to execute inside the WASM sandbox rather than as native Go code.

//go:embed wasm/*.wasm
var builtinWASMFS embed.FS

var (
	builtinWasmMu    sync.Mutex
	builtinWasmCache = map[string][]byte{}
)

func normalizeBuiltinWASMName(name string) string {
	n := strings.TrimSpace(strings.ToLower(name))
	switch n {
	case "minecraft_handshake", "minecraft", "mc":
		return "minecraft_handshake"
	case "tls_sni", "sni", "tls":
		return "tls_sni"
	default:
		return n
	}
}

func builtinWASMPath(name string) (string, bool) {
	switch normalizeBuiltinWASMName(name) {
	case "minecraft_handshake":
		return "wasm/minecraft_handshake.wasm", true
	case "tls_sni":
		return "wasm/tls_sni.wasm", true
	default:
		return "", false
	}
}

func builtinWASMBytes(ctx context.Context, name string) ([]byte, error) {
	name = normalizeBuiltinWASMName(name)

	builtinWasmMu.Lock()
	if b, ok := builtinWasmCache[name]; ok {
		builtinWasmMu.Unlock()
		return b, nil
	}
	builtinWasmMu.Unlock()

	p, ok := builtinWASMPath(name)
	if !ok {
		return nil, fmt.Errorf("protocol: unknown builtin wasm parser %q", name)
	}

	wasmBytes, err := builtinWASMFS.ReadFile(p)
	if err != nil {
		return nil, fmt.Errorf("protocol: read builtin wasm %q: %w", p, err)
	}

	builtinWasmMu.Lock()
	builtinWasmCache[name] = wasmBytes
	builtinWasmMu.Unlock()
	return wasmBytes, nil
}

// NewBuiltinWASMHostParser constructs a WASMHostParser backed by Prism's embedded
// builtin WASM implementations.
func NewBuiltinWASMHostParser(ctx context.Context, builtinName string, opts WASMHostParserOptions) (*WASMHostParser, error) {
	b, err := builtinWASMBytes(ctx, builtinName)
	if err != nil {
		return nil, err
	}
	pathHint := "builtin:" + normalizeBuiltinWASMName(builtinName)
	if opts.Name == "" {
		opts.Name = normalizeBuiltinWASMName(builtinName)
	}
	return NewWASMHostParser(ctx, b, pathHint, opts)
}
