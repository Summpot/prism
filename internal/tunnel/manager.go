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
	if err := writeProxyStreamHeader(st, service); err != nil {
		_ = st.Close()
		return nil, err
	}
	return st, nil
}

func (m *Manager) HasService(service string) bool {
	m.mu.RLock()
	_, ok := m.services[service]
	m.mu.RUnlock()
	return ok
}
