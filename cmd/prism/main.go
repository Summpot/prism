package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"log"
	"net/http"
	"os"
	"os/signal"
	"strings"
	"syscall"
	"time"

	"prism/internal/config"
	"prism/internal/protocol"
	"prism/internal/proxy"
	"prism/internal/router"
	"prism/internal/server"
	"prism/internal/telemetry"
)

type parserCloser func(context.Context) error

func buildHostParser(ctx context.Context, cfg *config.Config) (protocol.HostParser, parserCloser, error) {
	var parsers []protocol.HostParser
	var closers []parserCloser

	for _, pc := range cfg.RoutingParsers {
		t := strings.TrimSpace(strings.ToLower(pc.Type))
		if t == "" {
			t = "builtin"
		}
		switch t {
		case "builtin":
			name := strings.TrimSpace(strings.ToLower(pc.Name))
			switch name {
			case "minecraft_handshake", "minecraft", "mc":
				parsers = append(parsers, protocol.NewMinecraftHostParser())
			case "tls_sni", "sni", "tls":
				parsers = append(parsers, protocol.NewTLSSNIHostParser())
			default:
				return nil, nil, fmt.Errorf("unknown builtin routing parser %q", pc.Name)
			}
		case "wasm":
			if strings.TrimSpace(pc.Path) == "" {
				return nil, nil, fmt.Errorf("wasm routing parser missing path")
			}
			wp, err := protocol.NewWASMHostParserFromFile(ctx, pc.Path, protocol.WASMHostParserOptions{
				Name:         pc.Name,
				FunctionName: pc.Function,
				MaxOutputLen: uint32(pc.MaxOutputLen),
			})
			if err != nil {
				return nil, nil, err
			}
			parsers = append(parsers, wp)
			closers = append(closers, wp.Close)
		default:
			return nil, nil, fmt.Errorf("unknown routing parser type %q", pc.Type)
		}
	}

	chain := protocol.NewChainHostParser(parsers...)
	closeFn := parserCloser(func(ctx context.Context) error {
		var err error
		for _, c := range closers {
			if c == nil {
				continue
			}
			err = errors.Join(err, c(ctx))
		}
		return err
	})
	if len(closers) == 0 {
		closeFn = nil
	}
	return chain, closeFn, nil
}

func main() {
	var (
		configPath = flag.String("config", "config.json", "Path to Prism JSON config file")
	)
	flag.Parse()

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	provider := config.NewFileConfigProvider(*configPath)
	cfg, err := provider.Load(ctx)
	if err != nil {
		log.Fatalf("load config: %v", err)
	}

	cm := config.NewManager(provider, config.ManagerOptions{PollInterval: cfg.Reload.PollInterval})
	cm.SetCurrent(cfg)

	metrics := telemetry.NewMetricsCollector()
	sessions := proxy.NewSessionRegistry()

	r := router.NewRouter(cfg.Routes)
	h := proxy.NewSessionHandler(proxy.SessionHandlerOptions{})

	var currentClose parserCloser
	applyCfg := func(newCfg *config.Config) error {
		parser, closeFn, err := buildHostParser(ctx, newCfg)
		if err != nil {
			return err
		}
		// Update routes atomically.
		r.Update(newCfg.Routes)

		// Rotate dialer/bridge/parser for new sessions.
		dialer := proxy.NewNetDialer(&proxy.NetDialerOptions{Timeout: newCfg.UpstreamDialTimeout})
		bridge := proxy.NewProxyBridge(proxy.ProxyBridgeOptions{
			BufferPool:         proxy.NewSyncPoolBufferPool(newCfg.BufferSize),
			InjectProxyProtoV2: newCfg.ProxyProtocolV2,
			Metrics:            metrics,
		})

		h.Update(proxy.SessionHandlerOptions{
			Parser:         parser,
			Resolver:       r,
			Dialer:         dialer,
			Bridge:         bridge,
			Metrics:        metrics,
			Sessions:       sessions,
			Timeouts:       newCfg.Timeouts,
			MaxHeaderBytes: newCfg.MaxHeaderBytes,
		})

		// Retire old WASM parsers after the handshake window to avoid racing in-flight handshakes.
		oldClose := currentClose
		currentClose = closeFn
		if oldClose != nil {
			delay := newCfg.Timeouts.HandshakeTimeout
			if delay <= 0 {
				delay = 3 * time.Second
			}
			time.AfterFunc(2*delay, func() { _ = oldClose(context.Background()) })
		}
		return nil
	}

	// Initial apply.
	if err := applyCfg(cfg); err != nil {
		log.Fatalf("apply config: %v", err)
	}

	cm.Subscribe(func(_, newCfg *config.Config) {
		if err := applyCfg(newCfg); err != nil {
			log.Printf("apply config: %v", err)
		}
	})
	if cfg.Reload.Enabled {
		cm.Start(ctx)
	}

	tcpServer := server.NewTCPServer(cfg.ListenAddr, h, metrics)

	admin := telemetry.NewAdminServer(telemetry.AdminServerOptions{
		Addr:     cfg.AdminAddr,
		Metrics:  metrics,
		Sessions: sessions,
		Reload: func(ctx context.Context) error {
			return cm.ReloadNow(ctx)
		},
		Health: func() bool {
			return tcpServer.IsListening()
		},
	})

	go func() {
		if err := admin.Start(); err != nil && !errors.Is(err, http.ErrServerClosed) {
			log.Printf("admin server error: %v", err)
			stop()
		}
	}()

	go func() {
		if err := tcpServer.ListenAndServe(ctx); err != nil {
			log.Printf("tcp server error: %v", err)
			stop()
		}
	}()

	<-ctx.Done()

	shutdownCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	if err := admin.Shutdown(shutdownCtx); err != nil {
		log.Printf("admin shutdown: %v", err)
	}

	if err := tcpServer.Shutdown(shutdownCtx); err != nil {
		log.Printf("tcp shutdown: %v", err)
	}

	fmt.Println("prism exited")
}
