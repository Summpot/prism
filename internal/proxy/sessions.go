package proxy

import (
	"sync"
	"time"
)

type SessionInfo struct {
	ID        string    `json:"id"`
	Client    string    `json:"client"`
	Host      string    `json:"host"`
	Upstream  string    `json:"upstream"`
	StartedAt time.Time `json:"started_at"`
}

type SessionRegistry struct {
	mu       sync.Mutex
	sessions map[string]SessionInfo
}

func NewSessionRegistry() *SessionRegistry {
	return &SessionRegistry{sessions: map[string]SessionInfo{}}
}

func (r *SessionRegistry) Add(info SessionInfo) {
	r.mu.Lock()
	r.sessions[info.ID] = info
	r.mu.Unlock()
}

func (r *SessionRegistry) Remove(id string) {
	r.mu.Lock()
	delete(r.sessions, id)
	r.mu.Unlock()
}

func (r *SessionRegistry) Snapshot() []SessionInfo {
	r.mu.Lock()
	defer r.mu.Unlock()
	out := make([]SessionInfo, 0, len(r.sessions))
	for _, v := range r.sessions {
		out = append(out, v)
	}
	return out
}
