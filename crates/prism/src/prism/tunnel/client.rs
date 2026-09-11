//! Terminal client sidecar for Prism reverse proxy / tunnel system.
//!
//! Features:
//! - Zero configuration, automatic service catalog synchronization from Server.
//! - Optional Fake LAN multicast broadcaster (`fake_lan.rs`) for Minecraft.
//! - Local L7 ingress listener extracting host from Packet 0 using WASM protocol driver.
//! - PRPX proxy stream bridging with stateful optimizer (`optimizer.rs`).
//! - Exponential backoff reconnect loop on network interruption.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::RwLock;

use crate::prism::config::TunnelClientConfig;
use crate::prism::middleware::{
    FramePriority, HandshakeResult, PollResult, SessionState, StreamResult, WasmProtocolSession,
    compile_module_from_wat, get_default_middleware_wat, get_dynamic_middleware_config,
};
use crate::prism::net;
use crate::prism::tunnel::fake_lan::{AdvertisedService, FakeLanBroadcaster};
use crate::prism::tunnel::optimizer::{
    BatcherConfig, CompressorConfig, DEFAULT_BUFFER_THRESHOLD, DecompressorConfig, OptimizedReader,
    OptimizedWriter, OptimizerStats, OptimizerStatsSnapshot, SharedOptimizerStats,
    TrafficDirection, unix_ms,
};
use crate::prism::tunnel::protocol::{
    self, FLAG_OPTIMIZER, FLAG_RAW, ProxyStreamKind, RegisterRequest, RegisteredService,
};
use crate::prism::tunnel::transport::{
    QuicDialOptions, TransportDialOptions, TransportSession, WebSocketDialOptions,
};
use serde::{Deserialize, Serialize};

/// A single log entry captured from the client sidecar.
pub type ClientLogEntry = crate::prism::logging::LogEntry;

/// Terminal user sidecar client.
pub struct Client {
    config: TunnelClientConfig,
    middleware_dir: Option<PathBuf>,
    wasm_module: Option<Arc<wasmtime::Module>>,
    wasm_engine: wasmtime::Engine,
    known_services: Arc<RwLock<Vec<RegisteredService>>>,
    broadcaster: Option<Arc<FakeLanBroadcaster>>,
    current_sess: Arc<RwLock<Option<Arc<dyn TransportSession>>>>,
    dial_timeout: Duration,
    optimizer_stats: SharedOptimizerStats,
    active_transport: Arc<RwLock<Option<String>>>,
}

impl Client {
    /// Creates a new `Client` from [`TunnelClientConfig`].
    pub fn new(config: TunnelClientConfig) -> anyhow::Result<Self> {
        let wasm_engine = wasmtime::Engine::default();
        let wasm_module = Self::try_compile_middleware(&wasm_engine, &config.middleware, None)?;

        let broadcaster = if config.fake_lan_broadcast {
            Some(Arc::new(FakeLanBroadcaster::new()))
        } else {
            None
        };

        let optimizer_stats = Arc::new(OptimizerStats::new());

        Ok(Self {
            config,
            middleware_dir: None,
            wasm_module,
            wasm_engine,
            known_services: Arc::new(RwLock::new(Vec::new())),
            broadcaster,
            current_sess: Arc::new(RwLock::new(None)),
            dial_timeout: Duration::from_secs(5),
            optimizer_stats,
            active_transport: Arc::new(RwLock::new(None)),
        })
    }

    /// Sets the middleware directory used to look up `.wat` files.
    pub fn with_middleware_dir(mut self, dir: PathBuf) -> Self {
        if self.wasm_module.is_none() && self.config.middleware.is_some() {
            if let Ok(Some(module)) =
                Self::try_compile_middleware(&self.wasm_engine, &self.config.middleware, Some(&dir))
            {
                self.wasm_module = Some(module);
            }
        }
        self.middleware_dir = Some(dir);
        self
    }

    /// Sets dial timeout for connecting to tunnel server.
    #[allow(dead_code)]
    pub fn with_dial_timeout(mut self, timeout: Duration) -> Self {
        self.dial_timeout = timeout;
        self
    }

    /// Returns a reference to the active advertised services in the Fake LAN broadcaster, if enabled.
    #[allow(dead_code)]
    pub fn broadcaster(&self) -> Option<&Arc<FakeLanBroadcaster>> {
        self.broadcaster.as_ref()
    }

    /// Returns the currently known active services snapshot.
    pub async fn known_services(&self) -> Vec<RegisteredService> {
        self.known_services.read().await.clone()
    }

    /// Returns a reference to the optimizer stats accumulator.
    #[allow(dead_code)]
    pub fn optimizer_stats(&self) -> &SharedOptimizerStats {
        &self.optimizer_stats
    }

    /// Checks if currently connected to tunnel server.
    pub async fn is_connected(&self) -> bool {
        let current = self.current_sess.read().await;
        current.is_some()
    }

    /// Opens an in-band admin control stream targeting the server's local admin API.
    pub async fn open_admin_stream(
        &self,
    ) -> anyhow::Result<crate::prism::tunnel::transport::BoxedStream> {
        let sess = {
            let current = self.current_sess.read().await;
            current
                .as_ref()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("tunnel client: not connected to server"))?
        };

        let mut stream = sess.open_stream().await?;
        protocol::write_proxy_stream_header_with_flags(
            &mut stream,
            protocol::ProxyStreamKind::Tcp,
            protocol::ADMIN_SERVICE_NAME,
            protocol::FLAG_RAW,
        )
        .await?;

        Ok(stream)
    }

    /// Issues an HTTP request over an in-band `$admin` stream.
    pub async fn admin_http_request(
        &self,
        method: &str,
        path: &str,
        headers: &HashMap<String, String>,
        body: Option<&str>,
    ) -> anyhow::Result<(u16, String)> {
        let mut stream = self.open_admin_stream().await?;
        write_http_request(&mut stream, method, path, headers, body.unwrap_or("")).await?;
        read_http_response(&mut stream).await
    }

    /// Returns the active configuration.
    #[allow(dead_code)]
    pub fn config(&self) -> &TunnelClientConfig {
        &self.config
    }

    /// Returns a status snapshot of the client.
    pub async fn status(&self) -> ClientStatusSnapshot {
        let connected = self.is_connected().await;
        let services = self.known_services().await;
        let stats = self.optimizer_stats.snapshot();
        let active_proto = self.active_transport.read().await.clone();
        let display_transport = if connected {
            active_proto
                .clone()
                .unwrap_or_else(|| self.config.transport.clone())
        } else {
            self.config.transport.clone()
        };
        ClientStatusSnapshot {
            running: true,
            state: if connected {
                "connected".to_string()
            } else {
                "connecting".to_string()
            },
            server_addr: self.config.server_addr.clone(),
            transport: display_transport,
            actual_transport: active_proto,
            listen_addr: self.config.listen_addr.clone(),
            fake_lan_broadcast: self.config.fake_lan_broadcast,
            known_services: services,
            stats,
        }
    }

    fn try_compile_middleware(
        engine: &wasmtime::Engine,
        middleware_name: &Option<String>,
        middleware_dir: Option<&Path>,
    ) -> anyhow::Result<Option<Arc<wasmtime::Module>>> {
        let Some(name) = middleware_name else {
            return Ok(None);
        };
        let name = name.trim();
        if name.is_empty() {
            return Ok(None);
        }

        // 1. Direct file path
        let path = Path::new(name);
        if path.is_file() {
            let bytes = std::fs::read(path)?;
            let (module, _) = compile_module_from_wat(engine, name, &bytes)?;
            return Ok(Some(Arc::new(module)));
        }

        // 2. Lookup in middleware_dir
        if let Some(dir) = middleware_dir {
            let direct = dir.join(name);
            if direct.is_file() {
                let bytes = std::fs::read(&direct)?;
                let (module, _) = compile_module_from_wat(engine, name, &bytes)?;
                return Ok(Some(Arc::new(module)));
            }
            let with_ext = dir.join(format!("{name}.wat"));
            if with_ext.is_file() {
                let bytes = std::fs::read(&with_ext)?;
                let (module, _) = compile_module_from_wat(engine, name, &bytes)?;
                return Ok(Some(Arc::new(module)));
            }
        }

        // 3. Built-in default middlewares (e.g. "minecraft", "tls_sni")
        if let Some(wat) = get_default_middleware_wat(name) {
            let (module, _) = compile_module_from_wat(engine, name, wat.as_bytes())?;
            return Ok(Some(Arc::new(module)));
        }

        anyhow::bail!("middleware: unable to find or compile middleware module '{name}'")
    }

    /// Runs the terminal client sidecar.
    ///
    /// Binds the local TCP listener for players, runs the Fake LAN broadcaster,
    /// and maintains the tunnel connection to the server with exponential backoff.
    pub async fn run(
        &self,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        if self.config.server_addr.trim().is_empty() {
            anyhow::bail!("tunnel client: server_addr is required");
        }

        // 1. Bind local TCP listener for Minecraft players (primary on configured listen_addr)
        let bind_addr = net::normalize_bind_addr(&self.config.listen_addr);
        let listener = tokio::net::TcpListener::bind(&*bind_addr).await?;
        let local_port = listener.local_addr()?.port();

        // Also bind auxiliary loopback listeners on the same port:
        let mut listeners = vec![listener];

        // Ensure both IPv6 and IPv4 loopback are bound if possible
        if let Ok(l) = tokio::net::TcpListener::bind(format!("[::1]:{local_port}")).await {
            listeners.push(l);
        }
        if let Ok(l) = tokio::net::TcpListener::bind(format!("127.0.0.1:{local_port}")).await {
            listeners.push(l);
        }

        // Priority 1: 127.0.0.2 ..= 127.0.0.255
        for i in 2..=255 {
            let alias_addr = format!("127.0.0.{i}:{local_port}");
            if let Ok(l) = tokio::net::TcpListener::bind(&alias_addr).await {
                listeners.push(l);
            }
        }

        // Priority 2: 127.1.0.0 ..= 127.1.0.255 (within safe 127.1.x~127.7.x range)
        for i in 0..=255 {
            let alias_addr = format!("127.1.0.{i}:{local_port}");
            if let Ok(l) = tokio::net::TcpListener::bind(&alias_addr).await {
                listeners.push(l);
            }
        }

        tracing::info!(
            "Local listener bound to {} (and loopback aliases on port {}), targeting server {} via {}",
            bind_addr,
            local_port,
            self.config.server_addr,
            self.config.transport
        );

        // 2. Start Fake LAN broadcaster background task if enabled
        if let Some(broadcaster) = &self.broadcaster {
            tracing::info!("Minecraft Fake LAN auto-discovery service started");
            let broadcaster_clone = broadcaster.clone();
            let broadcaster_shutdown = shutdown.clone();
            tokio::spawn(async move {
                if let Err(err) = broadcaster_clone.run(broadcaster_shutdown.clone()).await {
                    tracing::warn!(err = %err, "tunnel client: fake lan broadcaster exited with error");
                }
            });
        }

        // 3. Spawn local player ingress accept loop across all bound loopback listeners
        let mut player_loop_handles = Vec::new();
        for l in listeners {
            let current_sess = self.current_sess.clone();
            let known_services = self.known_services.clone();
            let wasm_engine = self.wasm_engine.clone();
            let wasm_module = self.wasm_module.clone();
            let config = self.config.clone();
            let optimizer_stats = self.optimizer_stats.clone();
            let mut player_shutdown = shutdown.clone();

            let h = tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = player_shutdown.changed() => {
                            if *player_shutdown.borrow() {
                                break;
                            }
                        }
                        res = l.accept() => {
                            match res {
                                Ok((socket, peer_addr)) => {
                                    let current_sess = current_sess.clone();
                                    let known_services = known_services.clone();
                                    let wasm_engine = wasm_engine.clone();
                                    let wasm_module = wasm_module.clone();
                                    let config = config.clone();
                                    let optimizer_stats = optimizer_stats.clone();

                                    tokio::spawn(async move {
                                        if let Err(err) = handle_player_connection(
                                            socket,
                                            peer_addr,
                                            current_sess,
                                            known_services,
                                            wasm_engine,
                                            wasm_module,
                                            config,
                                            optimizer_stats,
                                        ).await {
                                            tracing::warn!(peer = %peer_addr, err = %err, "tunnel client: player connection error");
                                        }
                                    });
                                }
                                Err(err) => {
                                    tracing::warn!(err = %err, "tunnel client: accept error");
                                    tokio::time::sleep(Duration::from_millis(100)).await;
                                }
                            }
                        }
                    }
                }
            });
            player_loop_handles.push(h);
        }

        // 4. Run reconnect loop connecting to Server
        let mut backoff = Duration::from_secs(1);
        loop {
            if *shutdown.borrow() {
                break;
            }

            tracing::info!(
                "Connecting to tunnel server at {} via {}...",
                self.config.server_addr,
                self.config.transport
            );

            match self.connect_and_sync(local_port, shutdown.clone()).await {
                Ok(()) => {
                    tracing::info!("Tunnel session closed cleanly");
                    break;
                }
                Err(err) => {
                    tracing::warn!(
                        "Disconnected from server {}: {}; reconnecting in {:?}",
                        self.config.server_addr,
                        err,
                        backoff
                    );
                }
            }

            // Disconnected: clear active session, negotiated transport, and broadcaster list
            *self.current_sess.write().await = None;
            *self.active_transport.write().await = None;
            if let Some(broadcaster) = &self.broadcaster {
                broadcaster.clear().await;
            }

            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
                _ = tokio::time::sleep(backoff) => {}
            }

            backoff = (backoff * 2).min(Duration::from_secs(10));
        }

        for h in player_loop_handles {
            h.abort();
        }
        *self.current_sess.write().await = None;
        *self.active_transport.write().await = None;
        if let Some(broadcaster) = &self.broadcaster {
            broadcaster.clear().await;
        }

        Ok(())
    }

    /// Single connection attempt to Server:
    /// Dials transport, registers as "client", and receives dynamic service catalog updates.
    async fn connect_and_sync(
        &self,
        local_port: u16,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let dial_opts = TransportDialOptions {
            quic: QuicDialOptions {
                server_name: String::new(),
                insecure_skip_verify: true,
                next_protos: vec![],
            },
            websocket: WebSocketDialOptions {
                server_name: self
                    .config
                    .websocket
                    .as_ref()
                    .map(|w| w.server_name.clone())
                    .unwrap_or_default(),
                insecure_skip_verify: self
                    .config
                    .websocket
                    .as_ref()
                    .map(|w| w.insecure_skip_verify)
                    .unwrap_or(true),
            },
            webtransport: crate::prism::tunnel::transport::WebTransportDialOptions {
                server_name: String::new(),
                insecure_skip_verify: true,
            },
        };

        let doh_refs: Vec<&str> = self.config.doh_servers.iter().map(|s| s.as_str()).collect();
        let custom_doh = if doh_refs.is_empty() {
            None
        } else {
            Some(doh_refs.as_slice())
        };
        let candidates = crate::prism::tunnel::negotiator::resolve_candidates(
            &self.config.server_addr,
            Some(&self.config.transport),
            custom_doh,
        )
        .await?;

        let (sess, chosen) = crate::prism::tunnel::negotiator::dial_with_fallback(
            &candidates,
            self.dial_timeout,
            &dial_opts,
        )
        .await?;

        tracing::info!(
            server = %self.config.server_addr,
            protocol = %chosen.protocol,
            port = chosen.port,
            "Connected to server via negotiated protocol"
        );

        // Register as client
        let mut reg = sess.open_stream().await?;
        let req = RegisterRequest {
            client_type: "client".to_string(),
            token: self.config.auth_token.clone(),
            services: Vec::new(),
        };
        protocol::write_register_request(&mut reg, &req).await?;

        // Await initial service catalog update to confirm registration from the server.
        let initial_services = tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    sess.close().await;
                    return Ok(());
                }
                return Ok(());
            }
            res = protocol::read_service_catalog(&mut reg) => {
                match res {
                    Ok(s) => s,
                    Err(err) => {
                        anyhow::bail!(
                            "registration rejected by server (check auth_token or server ACLs) or catalog stream closed: {err}"
                        );
                    }
                }
            }
        };

        // Store active session and negotiated transport for player connections only after registration is accepted
        *self.current_sess.write().await = Some(sess.clone());
        *self.active_transport.write().await = Some(chosen.protocol.clone());

        tracing::info!(
            "Connected to server {} ({}), registered sidecar client",
            self.config.server_addr,
            self.config.transport
        );

        let names = initial_services
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        tracing::info!(
            "Received service catalog update: {} services [{}]",
            initial_services.len(),
            if names.is_empty() { "none" } else { &names }
        );

        // Update known services
        *self.known_services.write().await = initial_services.clone();

        // Update Fake LAN broadcaster
        if let Some(broadcaster) = &self.broadcaster {
            let mut advertised = Vec::with_capacity(initial_services.len());
            for s in &initial_services {
                advertised.push(AdvertisedService::new(
                    s.name.clone(),
                    local_port,
                    self.config.motd_prefix.clone(),
                ));
            }
            broadcaster.set_services(advertised).await;
        }

        // Catalog sync loop on the register stream for subsequent updates
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        sess.close().await;
                        return Ok(());
                    }
                }
                update = protocol::read_service_catalog(&mut reg) => {
                    let services = match update {
                        Ok(s) => s,
                        Err(err) => {
                            anyhow::bail!("catalog stream closed or failed: {err}");
                        }
                    };

                    let names = services.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ");
                    tracing::info!(
                        "Received service catalog update: {} services [{}]",
                        services.len(),
                        if names.is_empty() { "none" } else { &names }
                    );

                    // Update known services
                    *self.known_services.write().await = services.clone();

                    // Update Fake LAN broadcaster
                    if let Some(broadcaster) = &self.broadcaster {
                        let mut advertised = Vec::with_capacity(services.len());
                        for s in &services {
                            advertised.push(AdvertisedService::new(
                                s.name.clone(),
                                local_port,
                                self.config.motd_prefix.clone(),
                            ));
                        }
                        broadcaster.set_services(advertised).await;
                    }
                }
            }
        }
    }
}

/// Maps a 0-based service index to a loopback IPv4 address:
/// - index 0 -> 127.0.0.1
/// - index 1..=254 -> 127.0.0.2 ..= 127.0.0.255 (priority range)
/// - index 255.. -> 127.1.x.x ..= 127.7.x.x (safe range up to 127.7.x, higher may have OS issues)
#[allow(dead_code)]
pub fn loopback_ip_for_service_index(index: usize) -> Option<std::net::Ipv4Addr> {
    if index <= 254 {
        Some(std::net::Ipv4Addr::new(127, 0, 0, (index + 1) as u8))
    } else {
        let offset = index - 255;
        let b = 1 + (offset / 65536);
        if b > 7 {
            return None;
        }
        let rem = offset % 65536;
        let c = (rem / 256) as u8;
        let d = (rem % 256) as u8;
        Some(std::net::Ipv4Addr::new(127, b as u8, c, d))
    }
}

/// Decodes an IPv4 loopback address to a 0-based service index:
/// - 127.0.0.1 -> index 0
/// - 127.0.0.2 ..= 127.0.0.255 -> index 1 ..= 254
/// - 127.1.x.x ..= 127.7.x.x -> index 255..
/// Addresses outside this range (e.g. 127.8.x.x or higher) return None.
pub fn service_index_for_loopback_ip(ip: std::net::Ipv4Addr) -> Option<usize> {
    if !ip.is_loopback() {
        return None;
    }
    let octets = ip.octets();
    if octets[0] != 127 {
        return None;
    }
    if octets[1] == 0 && octets[2] == 0 {
        let d = octets[3] as usize;
        if d >= 1 {
            return Some(d - 1);
        }
        return None;
    }
    if (1..=7).contains(&octets[1]) {
        let b = octets[1] as usize;
        let c = octets[2] as usize;
        let d = octets[3] as usize;
        let offset = (b - 1) * 65536 + c * 256 + d;
        return Some(255 + offset);
    }
    None
}

/// Matches a requested host against known active services.
pub fn match_target_service(
    known: &[RegisteredService],
    host: Option<&str>,
) -> Option<RegisteredService> {
    if known.is_empty() {
        return None;
    }

    if let Some(host) = host {
        let raw = host.trim().trim_end_matches('.');
        let clean_host = if raw.starts_with('[') {
            if let Some(end) = raw.find(']') {
                raw[1..end].to_ascii_lowercase()
            } else {
                raw.to_ascii_lowercase()
            }
        } else if let Some((h, _)) = raw.rsplit_once(':') {
            if !h.contains(':') {
                h.to_ascii_lowercase()
            } else {
                raw.to_ascii_lowercase()
            }
        } else {
            raw.to_ascii_lowercase()
        };

        if !clean_host.is_empty() {
            // 1. Exact match with service name
            if let Some(svc) = known
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(&clean_host))
            {
                return Some(svc.clone());
            }

            // 2. Exact match with masquerade_host
            if let Some(svc) = known.iter().find(|s| {
                !s.masquerade_host.is_empty() && s.masquerade_host.eq_ignore_ascii_case(&clean_host)
            }) {
                return Some(svc.clone());
            }

            // 3. Loopback IP matching:
            // - IPv4: 127.0.0.1 -> 1st service, 127.0.0.2..=255 -> 2nd..255th, 127.1.x~127.7.x -> 256th..
            // - IPv6: ::1 or [::1] -> 1st service
            if let Ok(ip) = clean_host.parse::<std::net::IpAddr>() {
                match ip {
                    std::net::IpAddr::V4(v4) => {
                        if let Some(idx) = service_index_for_loopback_ip(v4) {
                            if idx < known.len() {
                                return Some(known[idx].clone());
                            }
                        }
                    }
                    std::net::IpAddr::V6(v6) => {
                        if v6.is_loopback() && !known.is_empty() {
                            return Some(known[0].clone());
                        }
                    }
                }
            } else {
                // 4. Subdomain prefix match (e.g. "survival.prism.local" -> "survival")
                if let Some((sub, _)) = clean_host.split_once('.') {
                    if let Some(svc) = known.iter().find(|s| s.name.eq_ignore_ascii_case(sub)) {
                        return Some(svc.clone());
                    }
                }
            }
        }
    }

    // 5. Default to first active service if only 1 service is registered
    if known.len() == 1 {
        return Some(known[0].clone());
    }

    None
}

async fn handle_player_connection(
    mut player_socket: tokio::net::TcpStream,
    peer_addr: std::net::SocketAddr,
    current_sess: Arc<RwLock<Option<Arc<dyn TransportSession>>>>,
    known_services: Arc<RwLock<Vec<RegisteredService>>>,
    wasm_engine: wasmtime::Engine,
    wasm_module: Option<Arc<wasmtime::Module>>,
    config: TunnelClientConfig,
    optimizer_stats: SharedOptimizerStats,
) -> anyhow::Result<()> {
    crate::prism::net::set_nodelay(&player_socket);
    tracing::info!(peer = %peer_addr, "Incoming player connection from {peer_addr}");

    // 1. Determine target service from handshake Packet 0 or default
    let (target_service, initial_bytes) = if let Some(module) = &wasm_module {
        let mut session = WasmProtocolSession::new(&wasm_engine, module)?;
        let mut buf = Vec::new();
        let mut temp = [0u8; 4096];

        let host = loop {
            let n = tokio::time::timeout(Duration::from_secs(5), player_socket.read(&mut temp))
                .await
                .map_err(|_| anyhow::anyhow!("timeout waiting for player handshake"))??;

            if n == 0 {
                anyhow::bail!("player disconnected before handshake");
            }
            buf.extend_from_slice(&temp[..n]);

            match session.poll(&buf)? {
                PollResult::Handshake(HandshakeResult::NeedMoreData) => {
                    if buf.len() > 64 * 1024 {
                        anyhow::bail!("handshake packet exceeded 64KB");
                    }
                    continue;
                }
                PollResult::Handshake(HandshakeResult::RouteMatch { host, rewrite }) => {
                    let initial = rewrite.unwrap_or(buf);
                    break (host, initial);
                }
                PollResult::Handshake(HandshakeResult::NoMatch) => {
                    anyhow::bail!("handshake did not match protocol");
                }
                PollResult::Stream(_) => {
                    anyhow::bail!("unexpected stream state during handshake");
                }
            }
        };

        let local_ip = player_socket.local_addr().ok().map(|a| a.ip().to_string());
        let known = known_services.read().await.clone();
        let matched = match_target_service(&known, host.0.as_deref())
            .or_else(|| match_target_service(&known, local_ip.as_deref()));
        let Some(svc) = matched else {
            anyhow::bail!(
                "no matching service found for host: {:?} (local: {:?})",
                host.0,
                local_ip
            );
        };
        (svc, host.1)
    } else {
        let local_ip = player_socket.local_addr().ok().map(|a| a.ip().to_string());
        let known = known_services.read().await.clone();
        let matched = match_target_service(&known, local_ip.as_deref())
            .or_else(|| match_target_service(&known, None));
        let Some(svc) = matched else {
            anyhow::bail!("no active services available to route player");
        };
        (svc, Vec::new())
    };

    tracing::info!(
        peer = %peer_addr,
        service = %target_service.name,
        "Player {peer_addr} routed to service '{}'",
        target_service.name
    );

    // 2. Obtain active tunnel transport session
    let sess = {
        let guard = current_sess.read().await;
        match guard.as_ref() {
            Some(s) => s.clone(),
            None => anyhow::bail!("tunnel client is not currently connected to server"),
        }
    };

    // 3. Open PRPX stream on transport session
    let mut prpx_stream = sess.open_stream().await?;

    let use_optimizer = config.optimizer.as_ref().map_or(false, |t| t.enabled)
        || target_service
            .optimizer
            .as_ref()
            .map_or(false, |t| t.enabled);

    let flags = if use_optimizer {
        FLAG_OPTIMIZER
    } else {
        FLAG_RAW
    };

    protocol::write_proxy_stream_header_with_flags(
        &mut prpx_stream,
        ProxyStreamKind::Tcp,
        &target_service.name,
        flags,
    )
    .await?;

    tracing::debug!(
        peer = %peer_addr,
        service = %target_service.name,
        optimizer = use_optimizer,
        "tunnel client: bridged to PRPX stream"
    );

    // 4. Bridge player socket <-> PRPX stream
    if use_optimizer {
        crate::prism::net::set_nodelay(&player_socket);
        let mut opt_cfg = target_service
            .optimizer
            .as_ref()
            .map(crate::prism::tunnel::optimizer::OptimizerConfig::from)
            .unwrap_or_else(|| {
                config
                    .optimizer
                    .as_ref()
                    .map(crate::prism::tunnel::optimizer::OptimizerConfig::from)
                    .unwrap_or(crate::prism::tunnel::optimizer::OptimizerConfig {
                        enabled: true,
                        ..Default::default()
                    })
            });
        opt_cfg.enabled = true;
        if let Some(client_opt) = config.optimizer.as_ref() {
            if let Some(w) = client_opt.zstd_window_log {
                opt_cfg.zstd_window_log = w;
                opt_cfg.zstd_window_log_downlink = w;
            }
            if let Some(ref path) = client_opt.zstd_dictionary {
                opt_cfg.dictionary = crate::prism::tunnel::optimizer::resolve_dictionary(
                    Some(path),
                    &target_service.name,
                );
            }
        }
        if opt_cfg.dictionary.is_none() {
            opt_cfg.dictionary =
                crate::prism::tunnel::optimizer::resolve_dictionary(None, &target_service.name);
        }
        let mw_name = config
            .middleware
            .clone()
            .or_else(|| target_service.middleware.clone());
        return crate::prism::tunnel::optimizer::pipeline::run_player_facing(
            prpx_stream,
            player_socket,
            opt_cfg,
            mw_name,
            None,
            vec![optimizer_stats.clone()],
            initial_bytes,
            Duration::ZERO,
            target_service.name.clone(),
            crate::prism::tunnel::optimizer::pipeline::ParamsRole::Opener,
        )
        .await;

        #[allow(unreachable_code, unused_variables, unused_mut, unused_assignments)]
        let zstd_window_log = config
            .optimizer
            .as_ref()
            .and_then(|t| t.zstd_window_log)
            .or_else(|| {
                target_service
                    .optimizer
                    .as_ref()
                    .and_then(|t| t.zstd_window_log)
            })
            .unwrap_or(23);

        let zstd_level = target_service
            .optimizer
            .as_ref()
            .and_then(|t| t.zstd_level)
            .unwrap_or(3);

        let flush_interval = target_service
            .optimizer
            .as_ref()
            .and_then(|t| t.flush_interval_ms)
            .map(Duration::from_millis)
            .unwrap_or_else(|| Duration::from_millis(20));

        let compressor_config = CompressorConfig {
            compression_level: zstd_level,
            window_log: zstd_window_log,
            dictionary: None,
        };
        let decompressor_config = DecompressorConfig {
            window_log: zstd_window_log,
            dictionary: None,
        };
        let batcher_config = BatcherConfig {
            flush_interval,
            buffer_threshold: DEFAULT_BUFFER_THRESHOLD,
        };

        let has_wasm = wasm_module.is_some();

        let (player_rd, mut player_wr) = player_socket.into_split();
        let (prpx_rd, prpx_wr) = tokio::io::split(prpx_stream);

        let mut opt_reader = OptimizedReader::new(prpx_rd, decompressor_config)?
            .with_direction(TrafficDirection::Downlink)
            .with_stats_and_raw_mode(optimizer_stats.clone(), !has_wasm);
        let mut opt_writer = OptimizedWriter::new(prpx_wr, batcher_config, compressor_config)?
            .with_direction(TrafficDirection::Uplink)
            .with_stats(optimizer_stats.clone());

        // Write initial handshake bytes if any
        if !initial_bytes.is_empty() {
            opt_writer.write_all(&initial_bytes).await?;
            opt_writer.flush().await?;
        }

        // Inbound: PRPX -> Player (decompress & generic WASM egress transform)
        let inbound = {
            let optimizer_stats = optimizer_stats.clone();
            let mut egress_session = if let Some(module) = &wasm_module {
                let mut s = WasmProtocolSession::new(&wasm_engine, module).ok();
                if let Some(ref mut sess) = s {
                    sess.set_state(SessionState::StreamingEgress);
                    if let Some(cfg) = get_dynamic_middleware_config("minecraft") {
                        let _ = sess.apply_config_map(&cfg);
                    }
                }
                s
            } else {
                None
            };
            async move {
                if let Some(ref mut sess) = egress_session {
                    let mut pending = Vec::new();
                    let mut buf = vec![0u8; 64 * 1024];
                    loop {
                        let n = opt_reader.read(&mut buf).await?;
                        if n == 0 {
                            break;
                        }
                        pending.extend_from_slice(&buf[..n]);
                        let written = sess
                            .process_egress_stream(&mut pending, &mut player_wr)
                            .await?;
                        optimizer_stats.add_direction_raw_bytes(
                            TrafficDirection::Downlink,
                            written as u64,
                            unix_ms(),
                        );
                    }
                    if !pending.is_empty() {
                        player_wr.write_all(&pending).await?;
                        optimizer_stats.add_direction_raw_bytes(
                            TrafficDirection::Downlink,
                            pending.len() as u64,
                            unix_ms(),
                        );
                    }
                } else {
                    tokio::io::copy(&mut opt_reader, &mut player_wr).await?;
                }
                player_wr.shutdown().await?;
                Ok::<(), anyhow::Error>(())
            }
        };

        let mut wasm_session = if let Some(module) = &wasm_module {
            let mut s = WasmProtocolSession::new(&wasm_engine, module).ok();
            if let Some(ref mut sess) = s {
                sess.set_state(SessionState::Streaming);
            }
            s
        } else {
            None
        };

        // Outbound: Player -> WasmProtocolSession (sniff keepalive) -> OptimizedWriter -> PRPX
        let outbound = async move {
            let mut player_rd = player_rd;
            let mut read_buf = Vec::with_capacity(64 * 1024);
            let mut tmp = [0u8; 8192];
            loop {
                let flush_dur = opt_writer.time_until_flush();
                tokio::select! {
                    res = player_rd.read(&mut tmp) => {
                        let n = res?;
                        if n == 0 {
                            if !read_buf.is_empty() {
                                opt_writer.write_frame(&read_buf, FramePriority::Defer).await?;
                                read_buf.clear();
                            }
                            opt_writer.flush_batch().await?;
                            opt_writer.shutdown().await?;
                            break;
                        }
                        read_buf.extend_from_slice(&tmp[..n]);

                        if let Some(ref mut sess) = wasm_session {
                            let mut offset = 0;
                            while offset < read_buf.len() {
                                let slice = &read_buf[offset..];
                                match sess.poll(slice) {
                                    Ok(PollResult::Stream(StreamResult::Frame {
                                        len,
                                        priority,
                                        payload,
                                    })) => {
                                        if len == 0 || len > slice.len() {
                                            break;
                                        }
                                        if let Some(ref payload) = payload {
                                            opt_writer
                                                .write_frame_with_metric(len, payload, priority)
                                                .await?;
                                        } else {
                                            opt_writer.write_frame(&slice[..len], priority).await?;
                                        }
                                        offset += len;
                                    }
                                    Ok(PollResult::Stream(StreamResult::NeedMoreData))
                                    | Ok(PollResult::Stream(StreamResult::Blocked)) => {
                                        break;
                                    }
                                    Ok(PollResult::Handshake(_)) => {
                                        opt_writer.write_frame(slice, FramePriority::Defer).await?;
                                        offset += slice.len();
                                        break;
                                    }
                                    Err(_) => {
                                        opt_writer.write_frame(slice, FramePriority::Defer).await?;
                                        offset += slice.len();
                                        break;
                                    }
                                }
                            }
                            if offset > 0 {
                                read_buf.drain(..offset);
                            }
                        } else {
                            opt_writer.write_frame(&read_buf, FramePriority::Defer).await?;
                            read_buf.clear();
                        }
                    }
                    _ = async {
                        if let Some(dur) = flush_dur {
                            tokio::time::sleep(dur).await;
                        } else {
                            std::future::pending::<()>().await;
                        }
                    } => {
                        opt_writer.flush_if_due().await?;
                    }
                }
            }
            opt_writer.flush_batch().await?;
            Ok::<(), anyhow::Error>(())
        };

        let mut in_fut = std::pin::pin!(inbound);
        let mut out_fut = std::pin::pin!(outbound);
        tokio::select! {
            res = &mut in_fut => { let _ = res; }
            res = &mut out_fut => { let _ = res; }
        }
    } else {
        if !initial_bytes.is_empty() {
            prpx_stream.write_all(&initial_bytes).await?;
        }
        let _ = tokio::io::copy_bidirectional(&mut player_socket, &mut prpx_stream).await;
    }

    Ok(())
}

/// Status snapshot for the terminal client sidecar.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClientStatusSnapshot {
    pub running: bool,
    pub state: String,
    pub server_addr: String,
    pub transport: String,
    #[serde(default)]
    pub actual_transport: Option<String>,
    pub listen_addr: String,
    pub fake_lan_broadcast: bool,
    pub known_services: Vec<RegisteredService>,
    pub stats: OptimizerStatsSnapshot,
}

impl Default for ClientStatusSnapshot {
    fn default() -> Self {
        Self {
            running: false,
            state: "idle".to_string(),
            server_addr: String::new(),
            transport: "quic".to_string(),
            actual_transport: None,
            listen_addr: "127.0.0.1:25565".to_string(),
            fake_lan_broadcast: true,
            known_services: Vec::new(),
            stats: OptimizerStatsSnapshot::default(),
        }
    }
}

struct ActiveClientInstance {
    client: Arc<Client>,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
}

/// Dynamic lifecycle controller for terminal client sidecar.
#[derive(Clone, Default)]
pub struct ClientController {
    active: Arc<RwLock<Option<ActiveClientInstance>>>,
    middleware_dir: Option<PathBuf>,
}

impl ClientController {
    /// Creates a new client controller.
    pub fn new(middleware_dir: Option<PathBuf>) -> Self {
        Self {
            active: Arc::new(RwLock::new(None)),
            middleware_dir,
        }
    }

    /// Attaches an already running Client instance.
    pub async fn attach(&self, client: Arc<Client>, shutdown_tx: tokio::sync::watch::Sender<bool>) {
        *self.active.write().await = Some(ActiveClientInstance {
            client,
            shutdown_tx,
        });
    }

    /// Starts or restarts the client sidecar with the given configuration.
    pub async fn start(&self, config: TunnelClientConfig) -> anyhow::Result<()> {
        self.stop().await;

        tracing::info!(
            "Starting client sidecar connecting to {}",
            config.server_addr
        );

        let mut client = Client::new(config)?;
        if let Some(ref dir) = self.middleware_dir {
            client = client.with_middleware_dir(dir.clone());
        }
        let client = Arc::new(client);
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let c = client.clone();
        tokio::spawn(async move {
            if let Err(err) = c.run(shutdown_rx).await {
                tracing::warn!(err = %err, "tunnel client: run exited with error");
            }
        });

        *self.active.write().await = Some(ActiveClientInstance {
            client,
            shutdown_tx,
        });

        Ok(())
    }

    /// Stops the currently running client sidecar, if any.
    pub async fn stop(&self) {
        let mut guard = self.active.write().await;
        if let Some(instance) = guard.take() {
            let _ = instance.shutdown_tx.send(true);
            tracing::info!("Client sidecar stopped");
        }
    }

    /// Returns a real-time status snapshot of the client.
    pub async fn status(&self) -> ClientStatusSnapshot {
        let guard = self.active.read().await;
        if let Some(instance) = guard.as_ref() {
            instance.client.status().await
        } else {
            ClientStatusSnapshot::default()
        }
    }

    /// Returns true if a client instance is actively running.
    #[allow(dead_code)]
    pub async fn is_running(&self) -> bool {
        self.active.read().await.is_some()
    }

    /// Returns recent logs up to `limit`.
    pub async fn logs(&self, limit: usize) -> Vec<ClientLogEntry> {
        crate::prism::logging::get_recent_logs(limit)
    }

    /// Clears all stored logs.
    pub async fn clear_logs(&self) {
        crate::prism::logging::clear_recent_logs();
    }

    /// Appends a log entry directly.
    #[allow(dead_code)]
    pub async fn add_log(&self, level: &str, target: &str, message: &str) {
        crate::prism::logging::append_log_entry(
            level.to_string(),
            target.to_string(),
            message.to_string(),
        );
    }

    /// Opens an in-band administrative stream to the connected tunnel server.
    #[allow(dead_code)]
    pub async fn open_admin_stream(
        &self,
    ) -> anyhow::Result<crate::prism::tunnel::transport::BoxedStream> {
        let guard = self.active.read().await;
        let client = match guard.as_ref() {
            Some(inst) => inst.client.clone(),
            None => anyhow::bail!("client sidecar is not running"),
        };
        client.open_admin_stream().await
    }

    /// HTTP request to the connected server's `$admin` API over the tunnel.
    pub async fn admin_http_request(
        &self,
        method: &str,
        path: &str,
        headers: &HashMap<String, String>,
        body: Option<&str>,
    ) -> anyhow::Result<(u16, String)> {
        let client = {
            let guard = self.active.read().await;
            match guard.as_ref() {
                Some(inst) => inst.client.clone(),
                None => anyhow::bail!("client sidecar is not running"),
            }
        };

        let mut last_err = anyhow::anyhow!("tunnel client: not connected to server");
        for attempt in 0..20 {
            match client.admin_http_request(method, path, headers, body).await {
                Ok(res) => return Ok(res),
                Err(err) => {
                    let msg = err.to_string();
                    last_err = err;
                    let retryable = msg.contains("not connected") || msg.contains("not running");
                    if !retryable || attempt == 19 {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(150)).await;
                }
            }
        }
        tracing::warn!(
            err = %last_err,
            method,
            path,
            "tunnel client: $admin HTTP request failed"
        );
        Err(last_err)
    }
}

async fn write_http_request<W: tokio::io::AsyncWrite + Unpin>(
    w: &mut W,
    method: &str,
    path: &str,
    headers: &HashMap<String, String>,
    body: &str,
) -> anyhow::Result<()> {
    let method = method.trim();
    let method = if method.is_empty() { "GET" } else { method };
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let body_bytes = body.as_bytes();

    w.write_all(format!("{method} {path} HTTP/1.1\r\n").as_bytes())
        .await?;

    let mut has_host = false;
    let mut has_connection = false;
    let mut has_content_length = false;
    let mut has_content_type = false;
    for (k, v) in headers {
        let key = k.trim();
        if key.is_empty() {
            continue;
        }
        let lower = key.to_ascii_lowercase();
        match lower.as_str() {
            "host" => has_host = true,
            "connection" => has_connection = true,
            "content-length" => has_content_length = true,
            "content-type" => has_content_type = true,
            "transfer-encoding" => continue,
            _ => {}
        }
        w.write_all(format!("{key}: {v}\r\n").as_bytes()).await?;
    }
    if !has_host {
        w.write_all(b"Host: 127.0.0.1\r\n").await?;
    }
    if !has_connection {
        w.write_all(b"Connection: close\r\n").await?;
    }
    if !has_content_type
        && !body_bytes.is_empty()
        && matches!(
            method.to_ascii_uppercase().as_str(),
            "POST" | "PUT" | "PATCH"
        )
    {
        w.write_all(b"Content-Type: application/json\r\n").await?;
    }
    if !has_content_length {
        w.write_all(format!("Content-Length: {}\r\n", body_bytes.len()).as_bytes())
            .await?;
    }
    w.write_all(b"\r\n").await?;
    if !body_bytes.is_empty() {
        w.write_all(body_bytes).await?;
    }
    w.flush().await?;
    let _ = w.shutdown().await;
    Ok(())
}

fn find_http_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4)
        .or_else(|| buf.windows(2).position(|w| w == b"\n\n").map(|i| i + 2))
}

fn header_value<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    let want = name.to_ascii_lowercase();
    for line in headers.lines() {
        let line = line.trim_end_matches('\r');
        if let Some((k, v)) = line.split_once(':')
            && k.trim().eq_ignore_ascii_case(&want)
        {
            return Some(v.trim());
        }
    }
    None
}

async fn read_http_response<R: tokio::io::AsyncRead + Unpin>(
    r: &mut R,
) -> anyhow::Result<(u16, String)> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 2048];
    let header_end = loop {
        let n = r.read(&mut tmp).await?;
        if n == 0 {
            anyhow::bail!("admin http: connection closed before response headers");
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_http_header_end(&buf) {
            break pos;
        }
        if buf.len() > 64 * 1024 {
            anyhow::bail!("admin http: response headers too large");
        }
    };

    let header_bytes = buf[..header_end].to_vec();
    let header_text = String::from_utf8_lossy(&header_bytes);
    let status_line = header_text.lines().next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| anyhow::anyhow!("admin http: malformed status line: {status_line}"))?;

    let mut body = buf[header_end..].to_vec();
    if let Some(len_s) = header_value(&header_text, "content-length") {
        let len: usize = len_s
            .parse()
            .map_err(|_| anyhow::anyhow!("admin http: invalid content-length"))?;
        while body.len() < len {
            let n = r.read(&mut tmp).await?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&tmp[..n]);
        }
        body.truncate(len);
    } else {
        loop {
            let n = r.read(&mut tmp).await?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&tmp[..n]);
            if body.len() > 8 * 1024 * 1024 {
                anyhow::bail!("admin http: response body too large");
            }
        }
    }

    Ok((status, String::from_utf8_lossy(&body).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_target_service_rules() {
        let services = vec![
            RegisteredService {
                name: "survival".into(),
                proto: "tcp".into(),
                local_addr: "127.0.0.1:25565".into(),
                route_only: false,
                remote_addr: "".into(),
                masquerade_host: "mc.prism.gg".into(),
                middleware: None,
                optimizer: None,
            },
            RegisteredService {
                name: "creative".into(),
                proto: "tcp".into(),
                local_addr: "127.0.0.1:25566".into(),
                route_only: false,
                remote_addr: "".into(),
                masquerade_host: "".into(),
                middleware: None,
                optimizer: None,
            },
        ];

        // 1. Direct name match
        let s = match_target_service(&services, Some("survival")).unwrap();
        assert_eq!(s.name, "survival");

        let s = match_target_service(&services, Some("SURVIVAL:25565")).unwrap();
        assert_eq!(s.name, "survival");

        // 2. Masquerade host match
        let s = match_target_service(&services, Some("mc.prism.gg")).unwrap();
        assert_eq!(s.name, "survival");

        // 3. Subdomain match
        let s = match_target_service(&services, Some("creative.prism.local")).unwrap();
        assert_eq!(s.name, "creative");

        // 4. Loopback IP matching: 127.0.0.1 -> index 0, 127.0.0.2 -> index 1
        let s = match_target_service(&services, Some("127.0.0.1")).unwrap();
        assert_eq!(s.name, "survival");
        let s = match_target_service(&services, Some("127.0.0.1:25565")).unwrap();
        assert_eq!(s.name, "survival");

        let s = match_target_service(&services, Some("127.0.0.2")).unwrap();
        assert_eq!(s.name, "creative");
        let s = match_target_service(&services, Some("127.0.0.2:25565")).unwrap();
        assert_eq!(s.name, "creative");

        // IPv6 loopback matching -> index 0 (survival)
        let s = match_target_service(&services, Some("::1")).unwrap();
        assert_eq!(s.name, "survival");
        let s = match_target_service(&services, Some("[::1]")).unwrap();
        assert_eq!(s.name, "survival");
        let s = match_target_service(&services, Some("[::1]:25565")).unwrap();
        assert_eq!(s.name, "survival");

        // 127.0.0.3 out of bounds for 2 services -> None
        assert!(match_target_service(&services, Some("127.0.0.3")).is_none());

        // 5. Unknown host with multiple services -> None
        assert!(match_target_service(&services, Some("unknown.host.com")).is_none());
        assert!(match_target_service(&services, None).is_none());

        // 6. Single service -> defaults even if host is None or unknown
        let single = vec![services[0].clone()];
        let s = match_target_service(&single, None).unwrap();
        assert_eq!(s.name, "survival");

        let s = match_target_service(&single, Some("127.0.0.1:25565")).unwrap();
        assert_eq!(s.name, "survival");
    }

    #[test]
    fn test_loopback_service_index_mapping() {
        use std::net::Ipv4Addr;

        // 127.0.0.1 -> index 0
        assert_eq!(
            loopback_ip_for_service_index(0).unwrap(),
            Ipv4Addr::new(127, 0, 0, 1)
        );
        assert_eq!(
            service_index_for_loopback_ip(Ipv4Addr::new(127, 0, 0, 1)),
            Some(0)
        );

        // 127.0.0.2 -> index 1
        assert_eq!(
            loopback_ip_for_service_index(1).unwrap(),
            Ipv4Addr::new(127, 0, 0, 2)
        );
        assert_eq!(
            service_index_for_loopback_ip(Ipv4Addr::new(127, 0, 0, 2)),
            Some(1)
        );

        // 127.0.0.255 -> index 254
        assert_eq!(
            loopback_ip_for_service_index(254).unwrap(),
            Ipv4Addr::new(127, 0, 0, 255)
        );
        assert_eq!(
            service_index_for_loopback_ip(Ipv4Addr::new(127, 0, 0, 255)),
            Some(254)
        );

        // 127.1.0.0 -> index 255
        assert_eq!(
            loopback_ip_for_service_index(255).unwrap(),
            Ipv4Addr::new(127, 1, 0, 0)
        );
        assert_eq!(
            service_index_for_loopback_ip(Ipv4Addr::new(127, 1, 0, 0)),
            Some(255)
        );

        // Roundtrip for higher indices up to 127.7.x
        let ip = loopback_ip_for_service_index(1000).unwrap();
        assert_eq!(service_index_for_loopback_ip(ip), Some(1000));

        // Outside safe range: 127.8.0.1 should not match any service index
        assert_eq!(
            service_index_for_loopback_ip(Ipv4Addr::new(127, 8, 0, 1)),
            None
        );
        assert_eq!(
            service_index_for_loopback_ip(Ipv4Addr::new(192, 168, 1, 1)),
            None
        );
    }

    #[test]
    fn test_client_middleware_compilation() {
        let cfg = TunnelClientConfig {
            server_addr: "127.0.0.1:7000".into(),
            transport: "tcp".into(),
            auth_token: "test-token".into(),
            listen_addr: "127.0.0.1:25565".into(),
            middleware: Some("minecraft".into()),
            fake_lan_broadcast: true,
            motd_prefix: "[Prism] ".into(),
            optimizer: None,
            websocket: None,
            doh_servers: Vec::new(),
        };

        let client = Client::new(cfg).expect("should initialize and compile builtin middleware");
        assert!(client.wasm_module.is_some());
        assert!(client.broadcaster.is_some());
    }

    #[tokio::test]
    async fn test_client_e2e_proxy_stream_and_optimizer() {
        use crate::prism::config::OptimizerClientConfig;
        use crate::prism::tunnel::manager::Manager;
        use crate::prism::tunnel::server::{QuicServerOptions, Server, ServerOptions};

        // 1. Start echo backend server
        let backend_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_addr = backend_listener.local_addr().unwrap().to_string();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = backend_listener.accept().await {
                tokio::spawn(async move {
                    let (mut rd, mut wr) = socket.split();
                    let _ = tokio::io::copy(&mut rd, &mut wr).await;
                });
            }
        });

        // 2. Start tunnel server
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap().to_string();
        drop(server_listener); // free for Server

        let mgr = Arc::new(Manager::new());
        let server = Server::new(ServerOptions {
            listen_addr: server_addr.clone(),
            transport: "tcp".into(),
            auth_token: "secret".into(),
            quic: QuicServerOptions {
                cert_file: "".into(),
                key_file: "".into(),
            },
            websocket: Default::default(),
            webtransport: Default::default(),
            manager: mgr.clone(),
            auth_manager: None,
            admin_addr: None,
        })
        .unwrap();

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let srv_shutdown = shutdown_rx.clone();
        tokio::spawn(async move {
            let _ = server.listen_and_serve(srv_shutdown).await;
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        // 3. Start Connector registering service "mc-echo"
        let connector = crate::prism::tunnel::connector::Connector::new(
            crate::prism::tunnel::connector::ConnectorOptions {
                server_addr: server_addr.clone(),
                transport: "tcp".into(),
                auth_token: "secret".into(),
                services: vec![RegisteredService {
                    name: "mc-echo".into(),
                    proto: "tcp".into(),
                    local_addr: backend_addr,
                    route_only: false,
                    remote_addr: "".into(),
                    masquerade_host: "".into(),
                    middleware: None,
                    optimizer: None,
                }],
                dial_timeout: Duration::from_secs(2),
                quic: crate::prism::tunnel::connector::QuicConnectorOptions {
                    server_name: "".into(),
                    insecure_skip_verify: true,
                },
                websocket: Default::default(),
                middleware_dir: None,
                optimizer: None,
                doh_servers: Vec::new(),
            },
        )
        .unwrap();

        let conn_shutdown = shutdown_rx.clone();
        tokio::spawn(async move {
            let _ = connector.run(conn_shutdown).await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        // 4. Start Client with optimizer enabled
        let client_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client_listen_addr = client_listener.local_addr().unwrap().to_string();
        drop(client_listener); // free for Client

        let client = Client::new(TunnelClientConfig {
            server_addr: server_addr.clone(),
            transport: "tcp".into(),
            auth_token: "secret".into(),
            listen_addr: client_listen_addr.clone(),
            middleware: None,
            fake_lan_broadcast: true,
            motd_prefix: "[Prism] ".into(),
            optimizer: Some(OptimizerClientConfig {
                enabled: true,
                zstd_window_log: Some(23),
                ..Default::default()
            }),
            websocket: None,
            doh_servers: Vec::new(),
        })
        .unwrap();

        let client_arc = Arc::new(client);
        let client_shutdown = shutdown_rx.clone();
        let c_clone = client_arc.clone();
        tokio::spawn(async move {
            let _ = c_clone.run(client_shutdown).await;
        });

        // Wait for client to connect and receive catalog
        let mut waited = 0;
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            waited += 50;
            if client_arc.known_services().await.len() >= 1 {
                break;
            }
            if waited > 8000 {
                panic!("timed out waiting for client to receive catalog");
            }
        }

        // Verify Fake LAN broadcaster received service
        let broadcaster = client_arc.broadcaster().expect("broadcaster enabled");
        let svcs = broadcaster.services().await;
        assert_eq!(svcs.len(), 1);
        assert_eq!(svcs[0].name, "mc-echo");

        // 5. Connect as Player to Client's listen_addr
        let mut player = tokio::net::TcpStream::connect(&client_listen_addr)
            .await
            .expect("player should connect to client listen_addr");

        // Send data
        let message = b"Hello from Minecraft Player via Prism optimized tunnel!";
        player.write_all(message).await.unwrap();

        let mut received = vec![0u8; message.len()];
        player.read_exact(&mut received).await.unwrap();
        assert_eq!(&received, message);

        // Shutdown everything
        shutdown_tx.send(true).unwrap();
    }

    #[tokio::test]
    async fn test_client_controller_lifecycle() {
        crate::prism::logging::init_desktop_or_test_subscriber();
        let controller = ClientController::new(None);
        let status = controller.status().await;
        assert!(!status.running);
        assert_eq!(status.state, "idle");
        assert!(!controller.is_running().await);

        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l);

        let cfg = TunnelClientConfig {
            server_addr: "127.0.0.1:9999".into(),
            transport: "tcp".into(),
            auth_token: "tok".into(),
            listen_addr: format!("127.0.0.1:{port}"),
            middleware: None,
            fake_lan_broadcast: false,
            motd_prefix: "".into(),
            optimizer: None,
            websocket: None,
            doh_servers: Vec::new(),
        };

        controller.start(cfg).await.unwrap();
        assert!(controller.is_running().await);

        let status = controller.status().await;
        assert!(status.running);
        assert_eq!(status.server_addr, "127.0.0.1:9999");
        assert_eq!(status.transport, "tcp");

        controller.stop().await;
        assert!(!controller.is_running().await);

        let logs = controller.logs(100).await;
        assert!(!logs.is_empty());
        assert!(logs.iter().any(|l| l.message.contains("client sidecar")));

        controller.clear_logs().await;
        let remaining = controller.logs(100).await;
        assert!(
            !remaining
                .iter()
                .any(|l| l.message.contains("client sidecar")),
            "logs before clear should not be present"
        );
    }

    #[tokio::test]
    async fn test_client_e2e_auto_adopt_service_optimizer() {
        use crate::prism::config::OptimizerConfig;
        use crate::prism::tunnel::manager::Manager;
        use crate::prism::tunnel::server::{QuicServerOptions, Server, ServerOptions};

        // 1. Backend server
        let backend_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_addr = backend_listener.local_addr().unwrap().to_string();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = backend_listener.accept().await {
                tokio::spawn(async move {
                    let (mut rd, mut wr) = socket.split();
                    let _ = tokio::io::copy(&mut rd, &mut wr).await;
                });
            }
        });

        // 2. Tunnel server
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap().to_string();
        drop(server_listener);

        let mgr = Arc::new(Manager::new());
        let server = Server::new(ServerOptions {
            listen_addr: server_addr.clone(),
            transport: "tcp".into(),
            auth_token: "secret".into(),
            quic: QuicServerOptions {
                cert_file: "".into(),
                key_file: "".into(),
            },
            websocket: Default::default(),
            webtransport: Default::default(),
            manager: mgr.clone(),
            auth_manager: None,
            admin_addr: None,
        })
        .unwrap();

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let srv_shutdown = shutdown_rx.clone();
        tokio::spawn(async move {
            let _ = server.listen_and_serve(srv_shutdown).await;
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        // 3. Connector registers service with optimizer ENABLED
        let connector = crate::prism::tunnel::connector::Connector::new(
            crate::prism::tunnel::connector::ConnectorOptions {
                server_addr: server_addr.clone(),
                transport: "tcp".into(),
                auth_token: "secret".into(),
                services: vec![RegisteredService {
                    name: "mc-opt".into(),
                    proto: "tcp".into(),
                    local_addr: backend_addr,
                    route_only: false,
                    remote_addr: "".into(),
                    masquerade_host: "".into(),
                    middleware: None,
                    optimizer: Some(OptimizerConfig {
                        enabled: true,
                        flush_interval_ms: Some(20),
                        zstd_window_log: Some(23),
                        zstd_level: Some(3),
                        ..Default::default()
                    }),
                }],
                dial_timeout: Duration::from_secs(2),
                quic: crate::prism::tunnel::connector::QuicConnectorOptions {
                    server_name: "".into(),
                    insecure_skip_verify: true,
                },
                websocket: Default::default(),
                middleware_dir: None,
                optimizer: None,
                doh_servers: Vec::new(),
            },
        )
        .unwrap();

        let conn_shutdown = shutdown_rx.clone();
        tokio::spawn(async move {
            let _ = connector.run(conn_shutdown).await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        // 4. Client starts with optimizer NONE (as in desktop UI client)
        let client_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client_listen_addr = client_listener.local_addr().unwrap().to_string();
        drop(client_listener);

        let client = Client::new(TunnelClientConfig {
            server_addr: server_addr.clone(),
            transport: "tcp".into(),
            auth_token: "secret".into(),
            listen_addr: client_listen_addr.clone(),
            middleware: None,
            fake_lan_broadcast: true,
            motd_prefix: "[Prism] ".into(),
            optimizer: None, // None! Must auto-adopt from service catalog
            websocket: None,
            doh_servers: Vec::new(),
        })
        .unwrap();

        let client_arc = Arc::new(client);
        let client_shutdown = shutdown_rx.clone();
        let c_clone = client_arc.clone();
        tokio::spawn(async move {
            let _ = c_clone.run(client_shutdown).await;
        });

        // Wait for client to connect and receive catalog
        let mut waited = 0;
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            waited += 50;
            if client_arc.is_connected().await {
                let svcs = client_arc.known_services().await;
                if svcs.iter().any(|s| s.name == "mc-opt") {
                    break;
                }
            }
            if waited > 8000 {
                panic!("timed out waiting for client to connect and receive catalog");
            }
        }

        // 5. Connect as Player to Client's listen_addr
        let mut player = tokio::net::TcpStream::connect(&client_listen_addr)
            .await
            .expect("player should connect to client listen_addr");

        // Send data
        let message = b"Hello from Minecraft Player via auto-adopted optimized tunnel!";
        player.write_all(message).await.unwrap();

        let mut received = vec![0u8; message.len()];
        player.read_exact(&mut received).await.unwrap();
        assert_eq!(&received, message);

        // Shutdown everything
        shutdown_tx.send(true).unwrap();
    }

    struct MockAdminSession {
        open_tx: tokio::sync::Mutex<
            tokio::sync::mpsc::Sender<crate::prism::tunnel::transport::BoxedStream>,
        >,
    }

    #[async_trait::async_trait]
    impl crate::prism::tunnel::transport::TransportSession for MockAdminSession {
        async fn open_stream(
            &self,
        ) -> anyhow::Result<crate::prism::tunnel::transport::BoxedStream> {
            let (client_side, server_side) = tokio::io::duplex(4096);
            self.open_tx
                .lock()
                .await
                .send(Box::new(server_side))
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(Box::new(client_side))
        }

        async fn accept_stream(
            &self,
        ) -> anyhow::Result<crate::prism::tunnel::transport::BoxedStream> {
            Err(anyhow::anyhow!("not implemented"))
        }

        async fn close(&self) {}
        fn remote_addr(&self) -> Option<std::net::SocketAddr> {
            None
        }
        fn local_addr(&self) -> Option<std::net::SocketAddr> {
            None
        }
    }

    #[tokio::test]
    async fn test_client_open_admin_stream() {
        use tokio::sync::mpsc;

        let (server_open_tx, mut server_open_rx) = mpsc::channel(16);
        let mock_sess = Arc::new(MockAdminSession {
            open_tx: tokio::sync::Mutex::new(server_open_tx),
        });

        let client = Client::new(TunnelClientConfig {
            server_addr: "127.0.0.1:12345".into(),
            transport: "tcp".into(),
            auth_token: "".into(),
            listen_addr: "127.0.0.1:0".into(),
            middleware: None,
            fake_lan_broadcast: false,
            motd_prefix: "".into(),
            optimizer: None,
            websocket: None,
            doh_servers: Vec::new(),
        })
        .unwrap();

        // Inject active session
        *client.current_sess.write().await = Some(mock_sess);

        // Spawn task to read from server side of opened stream
        let reader = tokio::spawn(async move {
            let mut st = server_open_rx.recv().await.expect("stream opened");
            let (kind, svc, flags) = protocol::read_proxy_stream_header_with_flags(&mut st)
                .await
                .expect("header read");
            assert_eq!(kind, protocol::ProxyStreamKind::Tcp);
            assert_eq!(svc, protocol::ADMIN_SERVICE_NAME);
            assert_eq!(flags, protocol::FLAG_RAW);
        });

        let _admin_stream = client
            .open_admin_stream()
            .await
            .expect("open_admin_stream success");
        reader.await.expect("reader passed");
    }

    #[tokio::test]
    async fn admin_http_request_roundtrip_over_duplex() {
        let (mut client_side, mut server_side) = tokio::io::duplex(4096);
        let server = tokio::spawn(async move {
            let mut buf = vec![0u8; 4096];
            let n = server_side.read(&mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..n]);
            assert!(req.contains("POST /auth/github/exchange HTTP/1.1"));
            assert!(req.contains("\"code\":\"abc\""));
            server_side
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 15\r\n\r\n{\"token\":\"t1\"}",
                )
                .await
                .unwrap();
            server_side.flush().await.unwrap();
        });

        let mut headers = HashMap::new();
        headers.insert("Content-Type".into(), "application/json".into());
        write_http_request(
            &mut client_side,
            "POST",
            "/auth/github/exchange",
            &headers,
            "{\"code\":\"abc\"}",
        )
        .await
        .unwrap();
        let (status, body) = read_http_response(&mut client_side).await.unwrap();
        assert_eq!(status, 200);
        assert_eq!(body, "{\"token\":\"t1\"}");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn test_client_e2e_in_band_admin_http() {
        use crate::prism::tunnel::manager::Manager;
        use crate::prism::tunnel::server::{QuicServerOptions, Server, ServerOptions};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let admin_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let admin_addr = admin_listener.local_addr().unwrap();

        tokio::spawn(async move {
            while let Ok((mut stream, _)) = admin_listener.accept().await {
                let mut buf = [0u8; 1024];
                if let Ok(n) = stream.read(&mut buf).await {
                    if String::from_utf8_lossy(&buf[..n]).contains("GET /health") {
                        let _ = stream
                            .write_all(
                                b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\n{\"ok\":true}",
                            )
                            .await;
                    }
                }
            }
        });

        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap().to_string();
        drop(server_listener);

        let mgr = Arc::new(Manager::new());
        let server = Server::new(ServerOptions {
            listen_addr: server_addr.clone(),
            transport: "tcp".into(),
            auth_token: "secret".into(),
            quic: QuicServerOptions::default(),
            websocket: Default::default(),
            webtransport: Default::default(),
            manager: mgr.clone(),
            auth_manager: None,
            admin_addr: Some(admin_addr),
        })
        .unwrap();

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let srv_shutdown = shutdown_rx.clone();
        tokio::spawn(async move {
            let _ = server.listen_and_serve(srv_shutdown).await;
        });

        let client_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client_listen_addr = client_listener.local_addr().unwrap().to_string();
        drop(client_listener);

        let client = Arc::new(
            Client::new(TunnelClientConfig {
                server_addr: server_addr.clone(),
                transport: "tcp".into(),
                auth_token: "secret".into(),
                listen_addr: client_listen_addr,
                middleware: None,
                fake_lan_broadcast: false,
                motd_prefix: "".into(),
                optimizer: None,
                websocket: None,
                doh_servers: Vec::new(),
            })
            .unwrap(),
        );

        let c_clone = client.clone();
        let client_shutdown = shutdown_rx.clone();
        tokio::spawn(async move {
            let _ = c_clone.run(client_shutdown).await;
        });

        let mut connected = false;
        for _ in 0..160 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if client.status().await.state == "connected" {
                connected = true;
                break;
            }
        }
        assert!(connected, "client should be connected");

        let (status, body) = client
            .admin_http_request("GET", "/health", &HashMap::new(), None)
            .await
            .expect("in-band $admin HTTP request");
        assert_eq!(status, 200);
        assert!(body.contains("{\"ok\":true}"));

        shutdown_tx.send(true).unwrap();
    }
}
