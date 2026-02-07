package app

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"net"
	"net/http"
	"strconv"
	"strings"
	"time"

	"prism/internal/config"
	"prism/internal/logging"
	"prism/internal/protocol"
	"prism/internal/proxy"
	"prism/internal/router"
	"prism/internal/server"
	"prism/internal/telemetry"
	"prism/internal/tunnel"
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

func RunPrisms(ctx context.Context, configPath string) error {
	return RunPrism(ctx, configPath)
}

func RunPrism(ctx context.Context, configPath string) error {
	runCtx, cancel := context.WithCancel(ctx)
	defer cancel()

	path := strings.TrimSpace(configPath)
	if path == "" {
		p, err := config.DiscoverConfigPath(".")
		if err != nil {
			return fmt.Errorf("discover config: %w", err)
		}
		path = p
	}

	provider := config.NewFileConfigProvider(path)
	cfg, err := provider.Load(runCtx)
	if err != nil {
		return fmt.Errorf("load config: %w", err)
	}

	logrt, err := logging.NewRuntime(cfg.Logging)
	if err != nil {
		return fmt.Errorf("init logging: %w", err)
	}
	defer func() { _ = logrt.Close() }()
	slog.SetDefault(logrt.Logger())
	logger := slog.Default()

	if !cfg.ServerEnabled && !cfg.TunnelClient.Enabled {
		return fmt.Errorf("config: nothing to run (set server_enabled=true and/or tunnel_client.enabled=true)")
	}
	logger.Info(
		"prism: starting",
		"config", path,
		"server_enabled", cfg.ServerEnabled,
		"tunnel_client_enabled", cfg.TunnelClient.Enabled,
		"listen_addr", cfg.ListenAddr,
		"admin_addr", cfg.AdminAddr,
	)

	// Tunnel client (optional) is created once; config changes require restart.
	if cfg.TunnelClient.Enabled {
		services := make([]tunnel.RegisteredService, 0, len(cfg.TunnelClient.Services))
		for _, s := range cfg.TunnelClient.Services {
			services = append(services, tunnel.RegisteredService{Name: s.Name, LocalAddr: s.LocalAddr})
		}
		client, err := tunnel.NewClient(tunnel.ClientOptions{
			ServerAddr:  cfg.TunnelClient.ServerAddr,
			Transport:   cfg.TunnelClient.Transport,
			AuthToken:   cfg.TunnelClient.AuthToken,
			Services:    services,
			DialTimeout: cfg.TunnelClient.DialTimeout,
			BufSize:     cfg.BufferSize,
			QUIC: tunnel.QUICDialOptions{
				ServerName:         cfg.TunnelClient.QUIC.ServerName,
				InsecureSkipVerify: cfg.TunnelClient.QUIC.InsecureSkipVerify,
			},
			Logger: logger,
		})
		if err != nil {
			return err
		}
		go func() {
			if err := client.Run(runCtx); err != nil && !errors.Is(err, context.Canceled) {
				logger.Error("tunnel client error", "err", err)
				cancel()
			}
		}()
	}

	if !cfg.ServerEnabled {
		if cfg.Tunnel.Enabled {
			logger.Warn("tunnel server is enabled but server_enabled=false; ignoring")
		}
		<-runCtx.Done()
		logger.Info("prism exited")
		return nil
	}

	cm := config.NewManager(provider, config.ManagerOptions{PollInterval: cfg.Reload.PollInterval, Logger: logger})
	cm.SetCurrent(cfg)

	metrics := telemetry.NewMetricsCollector()
	sessions := proxy.NewSessionRegistry()

	r := router.NewRouter(cfg.Routes)
	h := proxy.NewSessionHandler(proxy.SessionHandlerOptions{})

	// Tunnel server (optional) is created once; config changes require restart.
	var tunnelSrv *tunnel.Server
	var tunnelMgr *tunnel.Manager
	if cfg.Tunnel.Enabled {
		srv, err := tunnel.NewServer(tunnel.ServerOptions{
			Enabled:    cfg.Tunnel.Enabled,
			ListenAddr: cfg.Tunnel.ListenAddr,
			Transport:  cfg.Tunnel.Transport,
			AuthToken:  cfg.Tunnel.AuthToken,
			QUIC: tunnel.QUICOptions{
				CertFile: cfg.Tunnel.QUIC.CertFile,
				KeyFile:  cfg.Tunnel.QUIC.KeyFile,
			},
			Logger:  logger,
			Manager: nil,
		})
		if err != nil {
			return err
		}
		tunnelSrv = srv
		tunnelMgr = srv.Manager()
	}

	var currentClose parserCloser
	applyCfg := func(oldCfg, newCfg *config.Config) error {
		if oldCfg != nil && logrt.NeedsRestart(newCfg.Logging) {
			logger.Warn("logging config changed (restart required for format/output/buffer)")
		}
		if oldCfg != nil {
			if oldCfg.ServerEnabled != newCfg.ServerEnabled {
				logger.Warn("server_enabled changed (restart required)")
			}
			// Tunnel settings are not hot-reloadable.
			if oldCfg.Tunnel.Enabled != newCfg.Tunnel.Enabled || oldCfg.Tunnel.ListenAddr != newCfg.Tunnel.ListenAddr || oldCfg.Tunnel.Transport != newCfg.Tunnel.Transport {
				logger.Warn("tunnel config changed (restart required)")
			}
			if oldCfg.TunnelClient.Enabled != newCfg.TunnelClient.Enabled || oldCfg.TunnelClient.ServerAddr != newCfg.TunnelClient.ServerAddr || oldCfg.TunnelClient.Transport != newCfg.TunnelClient.Transport {
				logger.Warn("tunnel_client config changed (restart required)")
			}
		}
		if err := logrt.Apply(newCfg.Logging); err != nil {
			logger.Warn("apply logging config failed", "err", err)
		}

		parser, closeFn, err := buildHostParser(runCtx, newCfg)
		if err != nil {
			return err
		}
		// Update routes atomically.
		r.Update(newCfg.Routes)

		// Rotate dialer/bridge/parser for new sessions.
		baseDialer := proxy.NewNetDialer(&proxy.NetDialerOptions{Timeout: newCfg.UpstreamDialTimeout})
		dialer := proxy.Dialer(proxy.NewTunnelDialer(baseDialer, tunnelMgr))
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

	if err := applyCfg(nil, cfg); err != nil {
		return err
	}

	cm.Subscribe(func(oldCfg, newCfg *config.Config) {
		if err := applyCfg(oldCfg, newCfg); err != nil {
			logger.Error("apply config failed", "err", err)
		}
	})
	if cfg.Reload.Enabled {
		cm.Start(runCtx)
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

	if tunnelSrv != nil {
		go func() {
			if err := tunnelSrv.ListenAndServe(runCtx); err != nil && !errors.Is(err, context.Canceled) {
				logger.Error("tunnel server error", "err", err)
				cancel()
			}
		}()
	}

	go func() {
		if err := admin.Start(); err != nil && !errors.Is(err, http.ErrServerClosed) {
			logger.Error("admin server error", "err", err)
			cancel()
		}
	}()

	go func() {
		if err := tcpServer.ListenAndServe(runCtx); err != nil {
			logger.Error("tcp server error", "err", err)
			cancel()
		}
	}()

	<-runCtx.Done()

	shutdownCtx, cancelShutdown := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancelShutdown()

	if err := admin.Shutdown(shutdownCtx); err != nil {
		logger.Warn("admin shutdown", "err", err)
	}
	if err := tcpServer.Shutdown(shutdownCtx); err != nil {
		logger.Warn("tcp shutdown", "err", err)
	}
	if tunnelSrv != nil {
		if err := tunnelSrv.Shutdown(shutdownCtx); err != nil {
			logger.Warn("tunnel shutdown", "err", err)
		}
	}

	logger.Info("prism exited")
	return nil
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
