package protocol

import (
	"context"
	"errors"
	"fmt"
	"math"
	"os"
	"strings"
	"sync"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"
	"github.com/tetratelabs/wazero/imports/wasi_snapshot_preview1"
)

// WASMHostParser loads a HostParser implementation from a WebAssembly module.
//
// ABI (see DESIGN.md):
//  - module exports memory "memory"
//  - host writes input at offset 0
//  - exported function prism_parse(input_len:i32) -> i64
//		0  => need more
//		1  => no match
//		-1 => fatal error
//		otherwise => packed (ptr,len) where low32=ptr, high32=len
//
// Note: each Parse call uses a pooled module instance to allow concurrent use.
type WASMHostParser struct {
	name   string
	path   string
	fnName string

	rt       wazero.Runtime
	compiled wazero.CompiledModule

	pool sync.Pool // *wasmParserInstance

	maxOutputLen uint32
}

type WASMHostParserOptions struct {
	Name         string
	FunctionName string
	MaxOutputLen uint32
}

func NewWASMHostParserFromFile(ctx context.Context, path string, opts WASMHostParserOptions) (*WASMHostParser, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	return NewWASMHostParser(ctx, b, path, opts)
}

func NewWASMHostParser(ctx context.Context, wasmBytes []byte, pathHint string, opts WASMHostParserOptions) (*WASMHostParser, error) {
	fn := opts.FunctionName
	if fn == "" {
		fn = "prism_parse"
	}
	name := opts.Name
	if name == "" {
		name = "wasm"
		if pathHint != "" {
			name = "wasm:" + pathHint
		}
	}

	// One runtime per parser keeps the implementation simple and isolates plugins.
	rt := wazero.NewRuntime(ctx)
	wasi_snapshot_preview1.MustInstantiate(ctx, rt)

	compiled, err := rt.CompileModule(ctx, wasmBytes)
	if err != nil {
		_ = rt.Close(ctx)
		return nil, err
	}

	p := &WASMHostParser{
		name:         name,
		path:         pathHint,
		fnName:       fn,
		rt:           rt,
		compiled:     compiled,
		maxOutputLen: opts.MaxOutputLen,
	}
	if p.maxOutputLen == 0 {
		p.maxOutputLen = 255
	}

	p.pool.New = func() any {
		inst, _ := p.newInstance(context.Background())
		return inst
	}

	return p, nil
}

func (p *WASMHostParser) Name() string { return p.name }

func (p *WASMHostParser) Close(ctx context.Context) error {
	var err error
	if p.compiled != nil {
		err = errors.Join(err, p.compiled.Close(ctx))
	}
	if p.rt != nil {
		err = errors.Join(err, p.rt.Close(ctx))
	}
	return err
}

func (p *WASMHostParser) Parse(prelude []byte) (string, error) {
	instAny := p.pool.Get()
	inst, _ := instAny.(*wasmParserInstance)
	if inst == nil {
		var err error
		inst, err = p.newInstance(context.Background())
		if err != nil {
			return "", err
		}
	}

	host, err := p.parseWithInstance(context.Background(), inst, prelude)
	if err != nil {
		// If the instance errored, discard it.
		_ = inst.close(context.Background())
		return "", err
	}
	p.pool.Put(inst)
	return host, nil
}

type wasmParserInstance struct {
	m     api.Module
	mem   api.Memory
	parse api.Function
}

func (p *WASMHostParser) newInstance(ctx context.Context) (*wasmParserInstance, error) {
	m, err := p.rt.InstantiateModule(ctx, p.compiled, wazero.NewModuleConfig())
	if err != nil {
		return nil, err
	}
	mem := m.Memory()
	if mem == nil {
		_ = m.Close(ctx)
		return nil, fmt.Errorf("protocol: wasm parser %q has no exported memory", p.name)
	}
	fn := m.ExportedFunction(p.fnName)
	if fn == nil {
		_ = m.Close(ctx)
		return nil, fmt.Errorf("protocol: wasm parser %q missing export %q", p.name, p.fnName)
	}
	return &wasmParserInstance{m: m, mem: mem, parse: fn}, nil
}

func (i *wasmParserInstance) close(ctx context.Context) error {
	if i.m != nil {
		return i.m.Close(ctx)
	}
	return nil
}

func (p *WASMHostParser) parseWithInstance(ctx context.Context, inst *wasmParserInstance, prelude []byte) (string, error) {
	// Ensure memory can fit prelude at offset 0.
	// Memory size is in bytes, Grow is in 64KiB pages.
	need := uint32(len(prelude))
	memSize := inst.mem.Size()
	if need > memSize {
		pagesNeeded := (need - memSize + 65535) / 65536
		if _, ok := inst.mem.Grow(pagesNeeded); !ok {
			return "", fmt.Errorf("protocol: wasm memory grow failed")
		}
	}
	if len(prelude) > 0 {
		if !inst.mem.Write(0, prelude) {
			return "", fmt.Errorf("protocol: wasm memory write failed")
		}
	}

	res, err := inst.parse.Call(ctx, uint64(uint32(len(prelude))))
	if err != nil {
		return "", err
	}
	if len(res) != 1 {
		return "", fmt.Errorf("protocol: wasm parse returned %d values", len(res))
	}

	out := res[0]
	if out == 0 {
		return "", ErrNeedMoreData
	}
	if out == 1 {
		return "", ErrNoMatch
	}
	if out == math.MaxUint64 { // -1 as unsigned
		return "", fmt.Errorf("protocol: wasm parser fatal error")
	}

	ptr := uint32(out & 0xffffffff)
	ln := uint32(out >> 32)
	if ln == 0 {
		return "", ErrNoMatch
	}
	if ln > p.maxOutputLen {
		return "", fmt.Errorf("protocol: wasm hostname too long (%d)", ln)
	}
	b, ok := inst.mem.Read(ptr, ln)
	if !ok {
		return "", fmt.Errorf("protocol: wasm memory read failed")
	}
	host := strings.TrimSpace(strings.ToLower(string(b)))
	if host == "" {
		return "", ErrNoMatch
	}
	return host, nil
}

var _ HostParser = (*WASMHostParser)(nil)
