package telemetry

import (
	"sync"
	"sync/atomic"
)

type MetricsCollector struct {
	activeConnections atomic.Int64
	totalConnections  atomic.Int64
	bytesIngress      atomic.Int64
	bytesEgress       atomic.Int64

	routeMu   sync.Mutex
	routeHits map[string]int64
}

func NewMetricsCollector() *MetricsCollector {
	return &MetricsCollector{routeHits: map[string]int64{}}
}

func (m *MetricsCollector) IncActive() {
	m.activeConnections.Add(1)
	m.totalConnections.Add(1)
}

func (m *MetricsCollector) DecActive() {
	m.activeConnections.Add(-1)
}

func (m *MetricsCollector) AddIngress(n int64) {
	m.bytesIngress.Add(n)
}

func (m *MetricsCollector) AddEgress(n int64) {
	m.bytesEgress.Add(n)
}

func (m *MetricsCollector) AddRouteHit(host string) {
	m.routeMu.Lock()
	m.routeHits[host]++
	m.routeMu.Unlock()
}

type MetricsSnapshot struct {
	ActiveConnections int64            `json:"active_connections"`
	TotalConnections  int64            `json:"total_connections_handled"`
	BytesIngress      int64            `json:"bytes_ingress"`
	BytesEgress       int64            `json:"bytes_egress"`
	RouteHits         map[string]int64 `json:"route_hits"`
}

func (m *MetricsCollector) Snapshot() MetricsSnapshot {
	m.routeMu.Lock()
	rh := make(map[string]int64, len(m.routeHits))
	for k, v := range m.routeHits {
		rh[k] = v
	}
	m.routeMu.Unlock()

	return MetricsSnapshot{
		ActiveConnections: m.activeConnections.Load(),
		TotalConnections:  m.totalConnections.Load(),
		BytesIngress:      m.bytesIngress.Load(),
		BytesEgress:       m.bytesEgress.Load(),
		RouteHits:         rh,
	}
}
