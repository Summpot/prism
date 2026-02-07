package tunnel

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"net"
	"sync"
	"sync/atomic"
	"time"
)

var ErrServiceNotFound = errors.New("tunnel: service not found")

type Manager struct {
	mu sync.RWMutex

	clients map[string]*clientConn
	// service -> clientID
	services map[string]string

	logger *slog.Logger
	idSeq  atomic.Uint64
	subs   []func()
}

type clientConn struct {
	id       string
	sess     TransportSession
	services map[string]RegisteredService
	remote   string
	started  time.Time
}

func NewManager(logger *slog.Logger) *Manager {
	if logger == nil {
		logger = slog.Default()
	}
	return &Manager{
		clients:  map[string]*clientConn{},
		services: map[string]string{},
		logger:   logger,
	}
}

func (m *Manager) NextClientID(prefix string) string {
	if prefix == "" {
		prefix = "c"
	}
	return fmt.Sprintf("%s-%d", prefix, m.idSeq.Add(1))
}

func (m *Manager) RegisterClient(id string, sess TransportSession, services []RegisteredService) error {
	if id == "" {
		return fmt.Errorf("tunnel: empty client id")
	}
	if sess == nil {
		return fmt.Errorf("tunnel: nil session")
	}

	cc := &clientConn{
		id:       id,
		sess:     sess,
		services: map[string]RegisteredService{},
		started:  time.Now(),
	}
	if ra := sess.RemoteAddr(); ra != nil {
		cc.remote = ra.String()
	}
	for _, s := range services {
		if s.Name == "" {
			continue
		}
		cc.services[s.Name] = s
	}

	m.mu.Lock()
	defer m.mu.Unlock()

	// Replace any existing client with the same id.
	if old := m.clients[id]; old != nil {
		_ = old.sess.Close()
		for name := range old.services {
			if m.services[name] == id {
				delete(m.services, name)
			}
		}
	}
	m.clients[id] = cc

	// Last writer wins for service ownership.
	for name := range cc.services {
		m.services[name] = id
	}

	// Notify subscribers outside the lock.
	go m.notify()
	return nil
}

func (m *Manager) UnregisterClient(id string) {
	m.mu.Lock()
	defer m.mu.Unlock()

	cc := m.clients[id]
	if cc == nil {
		return
	}
	delete(m.clients, id)
	for name := range cc.services {
		if m.services[name] == id {
			delete(m.services, name)
		}
	}
	_ = cc.sess.Close()
	go m.notify()
}

// Subscribe registers a callback that is invoked whenever the service registry
// changes (client register/unregister). Callbacks may be called concurrently.
func (m *Manager) Subscribe(fn func()) {
	if fn == nil {
		return
	}
	m.mu.Lock()
	m.subs = append(m.subs, fn)
	m.mu.Unlock()
}

func (m *Manager) notify() {
	m.mu.RLock()
	subs := append([]func(){}, m.subs...)
	m.mu.RUnlock()
	for _, fn := range subs {
		if fn == nil {
			continue
		}
		fn()
	}
}

type ServiceSnapshot struct {
	Service  RegisteredService
	ClientID string
	Remote   string
}

func (m *Manager) SnapshotServices() []ServiceSnapshot {
	m.mu.RLock()
	defer m.mu.RUnlock()

	out := make([]ServiceSnapshot, 0, len(m.services))
	for name, cid := range m.services {
		cc := m.clients[cid]
		if cc == nil {
			continue
		}
		svc, ok := cc.services[name]
		if !ok {
			continue
		}
		out = append(out, ServiceSnapshot{Service: svc, ClientID: cid, Remote: cc.remote})
	}
	return out
}

func (m *Manager) DialService(ctx context.Context, service string) (net.Conn, error) {
	m.mu.RLock()
	cid, ok := m.services[service]
	cc := m.clients[cid]
	m.mu.RUnlock()
	if !ok || cc == nil {
		return nil, ErrServiceNotFound
	}

	st, err := cc.sess.OpenStream(ctx)
	if err != nil {
		return nil, err
	}
	if err := writeProxyStreamHeaderKind(st, ProxyStreamTCP, service); err != nil {
		_ = st.Close()
		return nil, err
	}
	return st, nil
}

// DialServiceUDP opens a tunnel stream for proxying UDP datagrams to the named
// service.
//
// The returned net.Conn implements datagram semantics via length-prefixed
// framing (see DatagramConn).
func (m *Manager) DialServiceUDP(ctx context.Context, service string) (net.Conn, error) {
	m.mu.RLock()
	cid, ok := m.services[service]
	cc := m.clients[cid]
	m.mu.RUnlock()
	if !ok || cc == nil {
		return nil, ErrServiceNotFound
	}

	st, err := cc.sess.OpenStream(ctx)
	if err != nil {
		return nil, err
	}
	if err := writeProxyStreamHeaderKind(st, ProxyStreamUDP, service); err != nil {
		_ = st.Close()
		return nil, err
	}
	return NewDatagramConn(st), nil
}

func (m *Manager) HasService(service string) bool {
	m.mu.RLock()
	_, ok := m.services[service]
	m.mu.RUnlock()
	return ok
}
