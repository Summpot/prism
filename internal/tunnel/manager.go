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
	// primary maps a service name to the clientID that should be used for
	// routing targets of the form tunnel:<service>.
	//
	// When multiple clients register the same service name, the first active
	// registrant remains primary (duplicates do not override routing).
	primary map[string]string

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
		clients: map[string]*clientConn{},
		primary: map[string]string{},
		logger:  logger,
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
			if m.primary[name] == id {
				delete(m.primary, name)
				m.promotePrimaryLocked(name)
			}
		}
	}
	m.clients[id] = cc

	// First writer wins for routing ownership; duplicates are kept but do not
	// override routing targets (tunnel:<service>).
	for name := range cc.services {
		if _, ok := m.primary[name]; !ok {
			m.primary[name] = id
		}
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
		if m.primary[name] == id {
			delete(m.primary, name)
			m.promotePrimaryLocked(name)
		}
	}
	_ = cc.sess.Close()
	go m.notify()
}

func (m *Manager) promotePrimaryLocked(serviceName string) {
	// Choose the oldest active client that provides this service.
	var chosenID string
	var chosenStarted time.Time
	for cid, cc := range m.clients {
		if cc == nil {
			continue
		}
		if _, ok := cc.services[serviceName]; !ok {
			continue
		}
		if chosenID == "" || cc.started.Before(chosenStarted) {
			chosenID = cid
			chosenStarted = cc.started
		}
	}
	if chosenID != "" {
		m.primary[serviceName] = chosenID
	}
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
	Primary  bool
}

func (m *Manager) SnapshotServices() []ServiceSnapshot {
	m.mu.RLock()
	defer m.mu.RUnlock()

	out := make([]ServiceSnapshot, 0, 16)
	for cid, cc := range m.clients {
		if cc == nil {
			continue
		}
		for name, svc := range cc.services {
			out = append(out, ServiceSnapshot{Service: svc, ClientID: cid, Remote: cc.remote, Primary: m.primary[name] == cid})
		}
	}
	return out
}

func (m *Manager) DialService(ctx context.Context, service string) (net.Conn, error) {
	m.mu.RLock()
	cid, ok := m.primary[service]
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

// DialServiceFromClient dials a specific client's registered service.
//
// This is used for per-service forwarding (auto-listen) so that services with
// duplicate names can be exposed by port without affecting routing.
func (m *Manager) DialServiceFromClient(ctx context.Context, clientID string, service string) (net.Conn, error) {
	m.mu.RLock()
	cc := m.clients[clientID]
	if cc == nil {
		m.mu.RUnlock()
		return nil, ErrServiceNotFound
	}
	_, ok := cc.services[service]
	m.mu.RUnlock()
	if !ok {
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
	cid, ok := m.primary[service]
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

// DialServiceUDPFromClient dials a specific client's registered service for
// UDP datagram proxying.
func (m *Manager) DialServiceUDPFromClient(ctx context.Context, clientID string, service string) (net.Conn, error) {
	m.mu.RLock()
	cc := m.clients[clientID]
	if cc == nil {
		m.mu.RUnlock()
		return nil, ErrServiceNotFound
	}
	_, ok := cc.services[service]
	m.mu.RUnlock()
	if !ok {
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
	_, ok := m.primary[service]
	m.mu.RUnlock()
	return ok
}
