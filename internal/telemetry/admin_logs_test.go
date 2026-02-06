package telemetry

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"prism/internal/proxy"
)

type fakeLogs struct {
	lines   []string
	dropped uint64
}

func (f fakeLogs) Snapshot(limit int) []string {
	if limit <= 0 || limit >= len(f.lines) {
		return append([]string{}, f.lines...)
	}
	return append([]string{}, f.lines[len(f.lines)-limit:]...)
}

func (f fakeLogs) Dropped() uint64 { return f.dropped }

func TestAdminServer_LogsEndpoint(t *testing.T) {
	as := &AdminServer{opts: AdminServerOptions{
		Metrics:  NewMetricsCollector(),
		Sessions: proxy.NewSessionRegistry(),
		Logs:     fakeLogs{lines: []string{"a", "b", "c"}, dropped: 2},
	}}

	ts := httptest.NewServer(newAdminMux(as))
	defer ts.Close()

	resp, err := http.Get(ts.URL + "/logs?limit=2")
	if err != nil {
		t.Fatalf("GET /logs: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("status=%d want=200", resp.StatusCode)
	}

	var out struct {
		Lines   []string `json:"lines"`
		Dropped uint64   `json:"dropped"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(out.Lines) != 2 || out.Lines[0] != "b" || out.Lines[1] != "c" {
		t.Fatalf("lines=%#v want [b c]", out.Lines)
	}
	if out.Dropped != 2 {
		t.Fatalf("dropped=%d want=2", out.Dropped)
	}
}

func TestAdminServer_LogsEndpointDisabled(t *testing.T) {
	as := &AdminServer{opts: AdminServerOptions{
		Metrics:  NewMetricsCollector(),
		Sessions: proxy.NewSessionRegistry(),
	}}

	ts := httptest.NewServer(newAdminMux(as))
	defer ts.Close()

	resp, err := http.Get(ts.URL + "/logs")
	if err != nil {
		t.Fatalf("GET /logs: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusNotFound {
		t.Fatalf("status=%d want=404", resp.StatusCode)
	}
}
