package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"log/slog"
	"net"
	"net/http"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"syscall"
	"time"

	"prism/internal/config"
	"prism/internal/logging"
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
		configPath = flag.String("config", "", "Path to Prism config file (.toml/.yaml/.yml/.json). If empty, auto-detect prism.toml > prism.yaml > prism.yml > prism.json")
	)
	flag.Parse()

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	path := strings.TrimSpace(*configPath)
	if path == "" {
		p, err := config.DiscoverConfigPath(".")
		if err != nil {
			_, _ = fmt.Fprintf(os.Stderr, "discover config: %v\n", err)
			os.Exit(1)
		}
		path = p
	}

	provider := config.NewFileConfigProvider(path)
	cfg, err := provider.Load(ctx)
	if err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "load config: %v\n", err)
		os.Exit(1)
	}

	logrt, err := logging.NewRuntime(cfg.Logging)
	if err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "init logging: %v\n", err)
		os.Exit(1)
	}
	defer func() { _ = logrt.Close() }()
	slog.SetDefault(logrt.Logger())
	logger := slog.Default()
	logger.Info("prism: starting", "config", path, "listen_addr", cfg.ListenAddr, "admin_addr", cfg.AdminAddr)

	cm := config.NewManager(provider, config.ManagerOptions{PollInterval: cfg.Reload.PollInterval, Logger: logger})
	cm.SetCurrent(cfg)

	metrics := telemetry.NewMetricsCollector()
	sessions := proxy.NewSessionRegistry()

	r := router.NewRouter(cfg.Routes)
	h := proxy.NewSessionHandler(proxy.SessionHandlerOptions{})

	var currentClose parserCloser
	applyCfg := func(oldCfg, newCfg *config.Config) error {
		if oldCfg != nil && logrt.NeedsRestart(newCfg.Logging) {
			logger.Warn("logging config changed (restart required for format/output/buffer)")
		}
		if err := logrt.Apply(newCfg.Logging); err != nil {
			logger.Warn("apply logging config failed", "err", err)
		}

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
		defaultUpstreamPort := defaultUpstreamPortFromListenAddr(newCfg.ListenAddr)

		h.Update(proxy.SessionHandlerOptions{
			Parser:              parser,
			Resolver:            r,
			Dialer:              dialer,
			Bridge:              bridge,
			Logger:              logger,
			Metrics:             metrics,
			Sessions:            sessions,
			Timeouts:            newCfg.Timeouts,
			MaxHeaderBytes:      newCfg.MaxHeaderBytes,
			DefaultUpstreamPort: defaultUpstreamPort,
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
	if err := applyCfg(nil, cfg); err != nil {
		logger.Error("apply config failed", "err", err)
		os.Exit(1)
	}

	cm.Subscribe(func(oldCfg, newCfg *config.Config) {
		if err := applyCfg(oldCfg, newCfg); err != nil {
			logger.Error("apply config failed", "err", err)
		}
	})
	if cfg.Reload.Enabled {
		cm.Start(ctx)
	}

	tcpServer := server.NewTCPServer(cfg.ListenAddr, h, metrics, logger)

	admin := telemetry.NewAdminServer(telemetry.AdminServerOptions{
		Addr:     cfg.AdminAddr,
		Metrics:  metrics,
		Sessions: sessions,
		Logs:     logrt.Store(),
		Reload: func(ctx context.Context) error {
			return cm.ReloadNow(ctx)
		},
		Health: func() bool {
			return tcpServer.IsListening()
		},
	})

	go func() {
		if err := admin.Start(); err != nil && !errors.Is(err, http.ErrServerClosed) {
			logger.Error("admin server error", "err", err)
			stop()
		}
	}()

	go func() {
		if err := tcpServer.ListenAndServe(ctx); err != nil {
			logger.Error("tcp server error", "err", err)
			stop()
		}
	}()

	<-ctx.Done()

	shutdownCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	if err := admin.Shutdown(shutdownCtx); err != nil {
		logger.Warn("admin shutdown", "err", err)
	}

	if err := tcpServer.Shutdown(shutdownCtx); err != nil {
		logger.Warn("tcp shutdown", "err", err)
	}
	logger.Info("prism exited")
}

func defaultUpstreamPortFromListenAddr(listenAddr string) int {
	listenAddr = strings.TrimSpace(listenAddr)
	if listenAddr == "" {
		return 25565
	}
	_, portStr, err := net.SplitHostPort(listenAddr)
	if err != nil {
		return 25565
	}
	p, err := strconv.Atoi(portStr)
	if err != nil || p <= 0 || p > 65535 {
		return 25565
	}
	return p
}
