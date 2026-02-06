package telemetry

import (
	"context"
	"encoding/json"
	"net/http"
	"time"

	"prism/internal/proxy"
)

type AdminServerOptions struct {
	Addr string

	Metrics  *MetricsCollector
	Sessions *proxy.SessionRegistry

	Reload func(ctx context.Context) error
	Health func() bool
}

type AdminServer struct {
	opts AdminServerOptions
	srv  *http.Server
}

func NewAdminServer(opts AdminServerOptions) *AdminServer {
	mux := http.NewServeMux()
	as := &AdminServer{opts: opts}

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

	as.srv = &http.Server{Addr: opts.Addr, Handler: mux}
	return as
}

func (a *AdminServer) Start() error {
	return a.srv.ListenAndServe()
}

func (a *AdminServer) Shutdown(ctx context.Context) error {
	return a.srv.Shutdown(ctx)
}
