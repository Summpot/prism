package telemetry

import (
	"context"
	"encoding/json"
	"net/http"
	"strconv"
	"time"

	"prism/internal/proxy"
)

type AdminServerOptions struct {
	Addr string

	Metrics  *MetricsCollector
	Sessions *proxy.SessionRegistry
	Logs     interface {
		Snapshot(limit int) []string
	}

	Reload func(ctx context.Context) error
	Health func() bool
}

type AdminServer struct {
	opts AdminServerOptions
	srv  *http.Server
}

func NewAdminServer(opts AdminServerOptions) *AdminServer {
	as := &AdminServer{opts: opts}
	as.srv = &http.Server{Addr: opts.Addr, Handler: newAdminMux(as)}
	return as
}

func newAdminMux(as *AdminServer) http.Handler {
	mux := http.NewServeMux()

	mux.HandleFunc("/health", func(w http.ResponseWriter, r *http.Request) {
		if as.opts.Health != nil && !as.opts.Health() {
			w.WriteHeader(http.StatusServiceUnavailable)
			return
		}
		w.WriteHeader(http.StatusOK)
	})

	mux.HandleFunc("/metrics", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(as.opts.Metrics.Snapshot())
	})

	mux.HandleFunc("/conns", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(as.opts.Sessions.Snapshot())
	})

	mux.HandleFunc("/logs", func(w http.ResponseWriter, r *http.Request) {
		if as.opts.Logs == nil {
			w.WriteHeader(http.StatusNotFound)
			return
		}
		limit := 200
		if v := r.URL.Query().Get("limit"); v != "" {
			if n, err := strconv.Atoi(v); err == nil {
				limit = n
			}
		}
		if limit <= 0 {
			limit = 200
		}
		if limit > 5000 {
			limit = 5000
		}
		resp := struct {
			Lines   []string `json:"lines"`
			Dropped uint64   `json:"dropped,omitempty"`
		}{
			Lines: as.opts.Logs.Snapshot(limit),
		}
		if d, ok := as.opts.Logs.(interface{ Dropped() uint64 }); ok {
			resp.Dropped = d.Dropped()
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(resp)
	})

	mux.HandleFunc("/reload", func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			w.WriteHeader(http.StatusMethodNotAllowed)
			return
		}
		if as.opts.Reload == nil {
			w.WriteHeader(http.StatusNotImplemented)
			return
		}
		ctx, cancel := context.WithTimeout(r.Context(), 5*time.Second)
		defer cancel()
		if err := as.opts.Reload(ctx); err != nil {
			w.WriteHeader(http.StatusBadRequest)
			_, _ = w.Write([]byte(err.Error()))
			return
		}
		w.WriteHeader(http.StatusOK)
	})

	return mux
}

func (a *AdminServer) Start() error {
	return a.srv.ListenAndServe()
}

func (a *AdminServer) Shutdown(ctx context.Context) error {
	return a.srv.Shutdown(ctx)
}
