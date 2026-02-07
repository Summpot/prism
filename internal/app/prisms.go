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

	resolved, err := config.ResolveConfigPath(configPath)
	if err != nil {
		return fmt.Errorf("resolve config path: %w", err)
	}
	path := resolved.Path

	created, err := config.EnsureConfigFile(path)
	if err != nil {
		return fmt.Errorf("ensure config file: %w", err)
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
	if created {
		logger.Warn("config: created new config file", "path", path, "source", resolved.Source)
	}

	proxyEnabled := len(cfg.Listeners) > 0
	tunnelServerEnabled := len(cfg.Tunnel.Listeners) > 0
	tunnelClientEnabled := cfg.Tunnel.Client != nil && strings.TrimSpace(cfg.Tunnel.Client.ServerAddr) != "" && len(cfg.Tunnel.Services) > 0
	adminEnabled := strings.TrimSpace(cfg.AdminAddr) != "" && (proxyEnabled || tunnelServerEnabled)

	if !proxyEnabled && !tunnelServerEnabled && !tunnelClientEnabled {
		return fmt.Errorf("config: nothing to run (set listeners and/or routes and/or tunnel.endpoints and/or tunnel.client+services)")
	}
	primaryListenAddr := ""
	if len(cfg.Listeners) > 0 {
		primaryListenAddr = cfg.Listeners[0].ListenAddr
	}
	logger.Info(
		"prism: starting",
		"config", path,
		"proxy_enabled", proxyEnabled,
		"tunnel_server_enabled", tunnelServerEnabled,
		"tunnel_client_enabled", tunnelClientEnabled,
		"listen_addr", primaryListenAddr,
		"admin_addr", cfg.AdminAddr,
		"tunnel_listeners", len(cfg.Tunnel.Listeners),
		"tunnel_services", len(cfg.Tunnel.Services),
		"proxy_listeners", len(cfg.Listeners),
	)

	// Tunnel client (optional) is created once; config changes require restart.
	if cfg.Tunnel.Client != nil && strings.TrimSpace(cfg.Tunnel.Client.ServerAddr) != "" {
		if len(cfg.Tunnel.Services) == 0 {
			logger.Warn("tunnel client configured but no services are registered; tunnel client will not start")
		} else {
			services := make([]tunnel.RegisteredService, 0, len(cfg.Tunnel.Services))
			for _, s := range cfg.Tunnel.Services {
				services = append(services, tunnel.RegisteredService{Name: s.Name, Proto: s.Proto, LocalAddr: s.LocalAddr, RouteOnly: s.RouteOnly, RemoteAddr: s.RemoteAddr})
			}
			client, err := tunnel.NewClient(tunnel.ClientOptions{
				ServerAddr:  cfg.Tunnel.Client.ServerAddr,
				Transport:   cfg.Tunnel.Client.Transport,
				AuthToken:   cfg.Tunnel.AuthToken,
				Services:    services,
				DialTimeout: cfg.Tunnel.Client.DialTimeout,
				BufSize:     cfg.BufferSize,
				QUIC: tunnel.QUICDialOptions{
					ServerName:         cfg.Tunnel.Client.QUIC.ServerName,
					InsecureSkipVerify: cfg.Tunnel.Client.QUIC.InsecureSkipVerify,
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
	}

	// Client-only mode: nothing else to start.
	if !proxyEnabled && !tunnelServerEnabled && !adminEnabled {
		<-runCtx.Done()
		logger.Info("prism exited")
		return nil
	}

	cm := config.NewManager(provider, config.ManagerOptions{PollInterval: cfg.Reload.PollInterval, Logger: logger})
	cm.SetCurrent(cfg)

	metrics := telemetry.NewMetricsCollector()
	sessions := proxy.NewSessionRegistry()

	r := router.NewRouter(cfg.Routes)

	type runningListener struct {
		cfg config.ProxyListenerConfig
		// Exactly one of these handlers is non-nil.
		routing *proxy.SessionHandler
		forward *proxy.ForwardHandler
		udp     *proxy.UDPForwarder

		tcp *server.TCPServer
		u   *server.UDPServer
	}
	var listeners []*runningListener
	if proxyEnabled {
		// Freeze listener topology at startup (changing listeners requires restart).
		for _, l := range cfg.Listeners {
			lc := l
			proto := strings.TrimSpace(strings.ToLower(lc.Protocol))
			if proto == "" {
				proto = "tcp"
			}
			lc.Protocol = proto
			up := strings.TrimSpace(lc.Upstream)
			lc.Upstream = up

			rl := &runningListener{cfg: lc}
			switch proto {
			case "tcp":
				if lc.Upstream == "" {
					rl.routing = proxy.NewSessionHandler(proxy.SessionHandlerOptions{})
					rl.tcp = server.NewTCPServer(lc.ListenAddr, rl.routing, metrics, logger)
				} else {
					rl.forward = proxy.NewForwardHandler(proxy.ForwardHandlerOptions{Network: "tcp", Upstream: lc.Upstream})
					rl.tcp = server.NewTCPServer(lc.ListenAddr, rl.forward, metrics, logger)
				}
			case "udp":
				rl.udp = proxy.NewUDPForwarder(proxy.UDPForwarderOptions{Upstream: lc.Upstream, IdleTimeout: cfg.Timeouts.IdleTimeout, Logger: logger})
				rl.u = server.NewUDPServer(lc.ListenAddr, rl.udp, logger)
			default:
				return fmt.Errorf("config: unsupported listener protocol %q", proto)
			}
			listeners = append(listeners, rl)
		}
	}

	// Tunnel server (optional) is created once; config changes require restart.
	var tunnelSrvs []*tunnel.Server
	var tunnelMgr *tunnel.Manager
	var svcAuto *tunnelServiceAutoListener
	if tunnelServerEnabled {
		tunnelMgr = tunnel.NewManager(logger)
		if cfg.Tunnel.AutoListenServices {
			svcAuto = newTunnelServiceAutoListener(runCtx, tunnelMgr, metrics, logger)
			// Reconcile on any registry change.
			tunnelMgr.Subscribe(func() {
				if svcAuto != nil {
					svcAuto.Reconcile()
				}
			})
		}
		for _, l := range cfg.Tunnel.Listeners {
			srv, err := tunnel.NewServer(tunnel.ServerOptions{
				ListenAddr: l.ListenAddr,
				Transport:  l.Transport,
				AuthToken:  cfg.Tunnel.AuthToken,
				QUIC: tunnel.QUICOptions{
					CertFile: l.QUIC.CertFile,
					KeyFile:  l.QUIC.KeyFile,
				},
				Logger:  logger,
				Manager: tunnelMgr,
			})
			if err != nil {
				return err
			}
			tunnelSrvs = append(tunnelSrvs, srv)
		}
	}

	var currentClose parserCloser
	applyCfg := func(oldCfg, newCfg *config.Config) error {
		if oldCfg != nil && logrt.NeedsRestart(newCfg.Logging) {
			logger.Warn("logging config changed (restart required for format/output/buffer)")
		}
		if oldCfg != nil {
			if !proxyListenersEqual(oldCfg.Listeners, newCfg.Listeners) {
				logger.Warn("listeners changed (restart required)")
			}
			if oldCfg.AdminAddr != newCfg.AdminAddr {
				logger.Warn("admin_addr changed (restart required)")
			}
			// Tunnel settings are not hot-reloadable.
			if !tunnelConfigEqual(oldCfg.Tunnel, newCfg.Tunnel) {
				logger.Warn("tunnel config changed (restart required)")
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

		// Update handlers for each configured listener.
		for _, rl := range listeners {
			if rl == nil {
				continue
			}
			switch rl.cfg.Protocol {
			case "tcp":
				if rl.routing != nil {
					defaultUpstreamPort := defaultUpstreamPortFromListenAddr(rl.cfg.ListenAddr)
					rl.routing.Update(proxy.SessionHandlerOptions{
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
				}
				if rl.forward != nil {
					rl.forward.Update(proxy.ForwardHandlerOptions{
						Network:  "tcp",
						Upstream: rl.cfg.Upstream,
						Dialer:   dialer,
						Bridge:   bridge,
						Logger:   logger,
						Timeouts: newCfg.Timeouts,
					})
				}
			case "udp":
				if rl.udp != nil {
					rl.udp.Update(proxy.UDPForwarderOptions{
						Upstream:    rl.cfg.Upstream,
						Dialer:      dialer,
						IdleTimeout: newCfg.Timeouts.IdleTimeout,
						Logger:      logger,
					})
				}
			}
		}
		if svcAuto != nil {
			svcAuto.UpdateRuntime(dialer, bridge, newCfg.Timeouts)
			// Initial reconcile after config/runtime is ready.
			svcAuto.Reconcile()
		}

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

	// Servers created earlier from cfg.Listeners.

	admin := telemetry.NewAdminServer(telemetry.AdminServerOptions{
		Addr:     cfg.AdminAddr,
		Metrics:  metrics,
		Sessions: sessions,
		Logs:     logrt.Store(),
		Reload: func(ctx context.Context) error {
			return cm.ReloadNow(ctx)
		},
		Health: func() bool {
			if proxyEnabled {
				for _, rl := range listeners {
					if rl == nil {
						continue
					}
					if rl.tcp != nil && rl.tcp.IsListening() {
						return true
					}
					if rl.u != nil && rl.u.IsListening() {
						return true
					}
				}
				return false
			}
			if len(tunnelSrvs) > 0 {
				for _, s := range tunnelSrvs {
					if s != nil && s.IsListening() {
						return true
					}
				}
				return false
			}
			return true
		},
	})

	for _, srv := range tunnelSrvs {
		s := srv
		go func() {
			if err := s.ListenAndServe(runCtx); err != nil && !errors.Is(err, context.Canceled) {
				logger.Error("tunnel server error", "err", err)
				cancel()
			}
		}()
	}

	if adminEnabled {
		go func() {
			if err := admin.Start(); err != nil && !errors.Is(err, http.ErrServerClosed) {
				logger.Error("admin server error", "err", err)
				cancel()
			}
		}()
	}

	if proxyEnabled {
		for _, rl := range listeners {
			s := rl
			if s == nil {
				continue
			}
			if s.tcp != nil {
				go func() {
					if err := s.tcp.ListenAndServe(runCtx); err != nil {
						logger.Error("tcp server error", "err", err)
						cancel()
					}
				}()
			}
			if s.u != nil {
				go func() {
					if err := s.u.ListenAndServe(runCtx); err != nil {
						logger.Error("udp server error", "err", err)
						cancel()
					}
				}()
			}
		}
	}

	<-runCtx.Done()

	shutdownCtx, cancelShutdown := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancelShutdown()

	if adminEnabled {
		if err := admin.Shutdown(shutdownCtx); err != nil {
			logger.Warn("admin shutdown", "err", err)
		}
	}
	for _, rl := range listeners {
		if rl == nil {
			continue
		}
		if rl.tcp != nil {
			if err := rl.tcp.Shutdown(shutdownCtx); err != nil {
				logger.Warn("tcp shutdown", "err", err)
			}
		}
		if rl.u != nil {
			if err := rl.u.Shutdown(shutdownCtx); err != nil {
				logger.Warn("udp shutdown", "err", err)
			}
		}
	}
	for _, srv := range tunnelSrvs {
		if srv == nil {
			continue
		}
		if err := srv.Shutdown(shutdownCtx); err != nil {
			logger.Warn("tunnel shutdown", "err", err)
		}
	}
	if svcAuto != nil {
		svcAuto.ShutdownAll(shutdownCtx)
	}

	logger.Info("prism exited")
	return nil
}

func tunnelConfigEqual(a, b config.TunnelConfig) bool {
	if a.AuthToken != b.AuthToken {
		return false
	}
	if a.AutoListenServices != b.AutoListenServices {
		return false
	}
	if (a.Client == nil) != (b.Client == nil) {
		return false
	}
	if a.Client != nil {
		if a.Client.ServerAddr != b.Client.ServerAddr || a.Client.Transport != b.Client.Transport || a.Client.DialTimeout != b.Client.DialTimeout {
			return false
		}
		if a.Client.QUIC != b.Client.QUIC {
			return false
		}
	}
	if len(a.Services) != len(b.Services) {
		return false
	}
	for i := range a.Services {
		if a.Services[i] != b.Services[i] {
			return false
		}
	}
	if len(a.Listeners) != len(b.Listeners) {
		return false
	}
	for i := range a.Listeners {
		la := a.Listeners[i]
		lb := b.Listeners[i]
		if la.ListenAddr != lb.ListenAddr || la.Transport != lb.Transport || la.QUIC != lb.QUIC {
			return false
		}
	}
	return true
}

func proxyListenersEqual(a, b []config.ProxyListenerConfig) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i].ListenAddr != b[i].ListenAddr || a[i].Protocol != b[i].Protocol || a[i].Upstream != b[i].Upstream {
			return false
		}
	}
	return true
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
