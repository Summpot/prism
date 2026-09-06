//! Terminal client sidecar for Prism reverse proxy / tunnel system.
//!
//! Features:
//! - Zero configuration, automatic service catalog synchronization from Server.
//! - Optional Fake LAN multicast broadcaster (`fake_lan.rs`) for Minecraft.
//! - Local L7 ingress listener extracting host from Packet 0 using WASM protocol driver.
//! - PRPX proxy stream bridging with stateful optimizer (`optimizer.rs`).
//! - Exponential backoff reconnect loop on network interruption.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::RwLock;

use crate::prism::config::TunnelClientConfig;
use crate::prism::middleware::{
    FramePriority, HandshakeResult, PollResult, SessionState, StreamResult, WasmProtocolSession,
    frame_uncompressed_packet, get_default_middleware_wat, recompress_packet_stream,
};
use crate::prism::net;
use crate::prism::tunnel::fake_lan::{AdvertisedService, FakeLanBroadcaster};
use crate::prism::tunnel::optimizer::{
    BatcherConfig, CompressorConfig, DEFAULT_BUFFER_THRESHOLD, DecompressorConfig, OptimizedReader,
    OptimizedWriter, OptimizerStats, OptimizerStatsSnapshot, SharedOptimizerStats,
    TrafficDirection,
};
use crate::prism::tunnel::protocol::{
    self, FLAG_OPTIMIZER, FLAG_RAW, ProxyStreamKind, RegisterRequest, RegisteredService,
};
use crate::prism::tunnel::transport::{
    QuicDialOptions, TransportDialOptions, TransportSession, WebSocketDialOptions,
    transport_by_name,
};
use serde::{Deserialize, Serialize};

/// A single log entry captured from the client sidecar.
pub type ClientLogEntry = crate::prism::logging::LogEntry;

/// Terminal user sidecar client.
pub struct Client {
    config: TunnelClientConfig,
    middleware_dir: Option<PathBuf>,
    wasm_module: Option<Arc<wasmer::Module>>,
    wasm_engine: wasmer::Engine,
    known_services: Arc<RwLock<Vec<RegisteredService>>>,
    broadcaster: Option<Arc<FakeLanBroadcaster>>,
    current_sess: Arc<RwLock<Option<Arc<dyn TransportSession>>>>,
    admin_bridge: Arc<RwLock<Option<AdminTunnelBridge>>>,
    dial_timeout: Duration,
    optimizer_stats: SharedOptimizerStats,
}

impl Client {
    /// Creates a new `Client` from [`TunnelClientConfig`].
    pub fn new(config: TunnelClientConfig) -> anyhow::Result<Self> {
        let wasm_engine = wasmer::Engine::default();
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
            admin_bridge: Arc::new(RwLock::new(None)),
            dial_timeout: Duration::from_secs(5),
            optimizer_stats,
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
        let admin_url = if let Some(b) = self.admin_bridge.read().await.as_ref() {
            Some(format!("http://{}", b.addr()))
        } else {
            None
        };
        ClientStatusSnapshot {
            running: true,
            state: if connected {
                "connected".to_string()
            } else {
                "connecting".to_string()
            },
            server_addr: self.config.server_addr.clone(),
            transport: self.config.transport.clone(),
            listen_addr: self.config.listen_addr.clone(),
            fake_lan_broadcast: self.config.fake_lan_broadcast,
            known_services: services,
            stats,
            admin_url,
        }
    }

    fn try_compile_middleware(
        engine: &wasmer::Engine,
        middleware_name: &Option<String>,
        middleware_dir: Option<&Path>,
    ) -> anyhow::Result<Option<Arc<wasmer::Module>>> {
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
            let store = wasmer::Store::new(engine.clone());
            let module = wasmer::Module::new(&store, bytes)?;
            return Ok(Some(Arc::new(module)));
        }

        // 2. Lookup in middleware_dir
        if let Some(dir) = middleware_dir {
            let direct = dir.join(name);
            if direct.is_file() {
                let bytes = std::fs::read(&direct)?;
                let store = wasmer::Store::new(engine.clone());
                let module = wasmer::Module::new(&store, bytes)?;
                return Ok(Some(Arc::new(module)));
            }
            let with_ext = dir.join(format!("{name}.wat"));
            if with_ext.is_file() {
                let bytes = std::fs::read(&with_ext)?;
                let store = wasmer::Store::new(engine.clone());
                let module = wasmer::Module::new(&store, bytes)?;
                return Ok(Some(Arc::new(module)));
            }
        }

        // 3. Built-in default middlewares (e.g. "minecraft", "tls_sni")
        if let Some(wat) = get_default_middleware_wat(name) {
            let store = wasmer::Store::new(engine.clone());
            let module = wasmer::Module::new(&store, wat.as_bytes())?;
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

        // 1. Bind local TCP listener for Minecraft players
        let bind_addr = net::normalize_bind_addr(&self.config.listen_addr);
        let listener = tokio::net::TcpListener::bind(&*bind_addr).await?;
        let local_port = listener.local_addr()?.port();

        tracing::info!(
            "Local listener bound to {} (port {}), targeting server {} via {}",
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

        // 3. Spawn local player ingress accept loop
        let player_loop_handle = {
            let current_sess = self.current_sess.clone();
            let known_services = self.known_services.clone();
            let wasm_engine = self.wasm_engine.clone();
            let wasm_module = self.wasm_module.clone();
            let config = self.config.clone();
            let optimizer_stats = self.optimizer_stats.clone();
            let mut player_shutdown = shutdown.clone();

            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = player_shutdown.changed() => {
                            if *player_shutdown.borrow() {
                                break;
                            }
                        }
                        res = listener.accept() => {
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
            })
        };

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

            // Disconnected: clear active session and clear broadcaster list
            *self.current_sess.write().await = None;
            if let Some(b) = self.admin_bridge.write().await.take() {
                b.close();
            }
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

        player_loop_handle.abort();
        *self.current_sess.write().await = None;
        if let Some(b) = self.admin_bridge.write().await.take() {
            b.close();
        }
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
        let tr = transport_by_name(&self.config.transport)?;

        let dial = async {
            tr.dial(
                &self.config.server_addr,
                TransportDialOptions {
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
                },
            )
            .await
        };

        let sess = tokio::time::timeout(self.dial_timeout, dial).await??;

        // Register as client
        let mut reg = sess.open_stream().await?;
        let req = RegisterRequest {
            client_type: "client".to_string(),
            token: self.config.auth_token.clone(),
            services: Vec::new(),
        };
        protocol::write_register_request(&mut reg, &req).await?;

        // Store active session for player connections
        *self.current_sess.write().await = Some(sess.clone());

        match AdminTunnelBridge::start(self.current_sess.clone()).await {
            Ok(bridge) => {
                tracing::info!(
                    admin_url = %format!("http://{}", bridge.addr()),
                    "Tunnel admin bridge listening locally"
                );
                *self.admin_bridge.write().await = Some(bridge);
            }
            Err(err) => {
                tracing::warn!(err = %err, "Failed to start local admin bridge");
            }
        }

        tracing::info!(
            "Connected to server {} ({}), registered sidecar client",
            self.config.server_addr,
            self.config.transport
        );

        // Catalog sync loop on the register stream
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

/// Matches a requested host against known active services.
pub fn match_target_service(
    known: &[RegisteredService],
    host: Option<&str>,
) -> Option<RegisteredService> {
    if known.is_empty() {
        return None;
    }

    if let Some(host) = host {
        let clean_host = host
            .trim()
            .trim_end_matches('.')
            .split(':')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();

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

            // 3. Subdomain prefix match (e.g. "survival.prism.local" -> "survival")
            if let Some((sub, _)) = clean_host.split_once('.') {
                if let Some(svc) = known.iter().find(|s| s.name.eq_ignore_ascii_case(sub)) {
                    return Some(svc.clone());
                }
            }
        }
    }

    // 4. Default to first active service if only 1 service is registered
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
    wasm_engine: wasmer::Engine,
    wasm_module: Option<Arc<wasmer::Module>>,
    config: TunnelClientConfig,
    optimizer_stats: SharedOptimizerStats,
) -> anyhow::Result<()> {
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

        let known = known_services.read().await.clone();
        let matched = match_target_service(&known, host.0.as_deref());
        let Some(svc) = matched else {
            anyhow::bail!("no matching service found for host: {:?}", host.0);
        };
        (svc, host.1)
    } else {
        let known = known_services.read().await.clone();
        let matched = match_target_service(&known, None);
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
        };
        let decompressor_config = DecompressorConfig {
            window_log: zstd_window_log,
        };
        let batcher_config = BatcherConfig {
            flush_interval,
            buffer_threshold: DEFAULT_BUFFER_THRESHOLD,
        };

        let is_minecraft = wasm_module.is_some();

        let (player_rd, mut player_wr) = player_socket.into_split();
        let (prpx_rd, prpx_wr) = tokio::io::split(prpx_stream);

        let mut opt_reader = OptimizedReader::new(prpx_rd, decompressor_config)?
            .with_direction(TrafficDirection::Downlink)
            .with_stats_and_raw_mode(optimizer_stats.clone(), !is_minecraft);
        let mut opt_writer = OptimizedWriter::new(prpx_wr, batcher_config, compressor_config)?
            .with_direction(TrafficDirection::Uplink)
            .with_stats(optimizer_stats.clone());

        // Write initial handshake bytes if any
        if !initial_bytes.is_empty() {
            opt_writer.write_all(&initial_bytes).await?;
            opt_writer.flush().await?;
        }

        // Inbound: PRPX -> Player (decompress)
        let inbound = {
            let optimizer_stats = optimizer_stats.clone();
            async move {
                if is_minecraft {
                    let mut pending = Vec::new();
                    let mut buf = vec![0u8; 64 * 1024];
                    loop {
                        let n = opt_reader.read(&mut buf).await?;
                        if n == 0 {
                            break;
                        }
                        pending.extend_from_slice(&buf[..n]);
                        let written =
                            recompress_packet_stream(&mut pending, &mut player_wr, 256).await?;
                        optimizer_stats
                            .add_direction_raw_bytes(TrafficDirection::Downlink, written as u64);
                    }
                    if !pending.is_empty() {
                        player_wr.write_all(&pending).await?;
                        optimizer_stats.add_direction_raw_bytes(
                            TrafficDirection::Downlink,
                            pending.len() as u64,
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
                                        if let Some(ref decompressed) = payload {
                                            let framed = frame_uncompressed_packet(decompressed);
                                            opt_writer
                                                .write_frame_with_metric(len, &framed, priority)
                                                .await?;
                                        } else {
                                            opt_writer.write_frame(&slice[..len], priority).await?;
                                        }
                                        offset += len;
                                    }
                                    Ok(PollResult::Stream(StreamResult::NeedMoreData)) => {
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

/// Local loopback TCP bridge that tunnels incoming HTTP connections to the server's admin service.
pub struct AdminTunnelBridge {
    listen_addr: std::net::SocketAddr,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
}

impl AdminTunnelBridge {
    pub async fn start(
        current_sess: Arc<RwLock<Option<Arc<dyn TransportSession>>>>,
    ) -> anyhow::Result<Self> {
        let listener = match tokio::net::TcpListener::bind("127.0.0.1:18080").await {
            Ok(l) => l,
            Err(_) => tokio::net::TcpListener::bind("127.0.0.1:0").await?,
        };
        let listen_addr = listener.local_addr()?;
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);

        let sess_holder = current_sess.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                    res = listener.accept() => {
                        match res {
                            Ok((mut local_stream, _)) => {
                                let sess_guard = sess_holder.read().await;
                                let sess = match sess_guard.as_ref() {
                                    Some(s) => s.clone(),
                                    None => continue,
                                };
                                drop(sess_guard);

                                tokio::spawn(async move {
                                    match sess.open_stream().await {
                                        Ok(mut admin_stream) => {
                                            if let Err(err) = protocol::write_proxy_stream_header_with_flags(
                                                &mut admin_stream,
                                                protocol::ProxyStreamKind::Tcp,
                                                protocol::ADMIN_SERVICE_NAME,
                                                protocol::FLAG_RAW,
                                            ).await {
                                                tracing::warn!(err = %err, "admin bridge: failed to write header");
                                                return;
                                            }
                                            let _ = tokio::io::copy_bidirectional(&mut local_stream, &mut admin_stream).await;
                                        }
                                        Err(err) => {
                                            tracing::warn!(err = %err, "admin bridge: failed to open stream to server");
                                        }
                                    }
                                });
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
        });

        Ok(Self {
            listen_addr,
            shutdown_tx,
        })
    }

    pub fn addr(&self) -> std::net::SocketAddr {
        self.listen_addr
    }

    pub fn close(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}

/// Status snapshot for the terminal client sidecar.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClientStatusSnapshot {
    pub running: bool,
    pub state: String,
    pub server_addr: String,
    pub transport: String,
    pub listen_addr: String,
    pub fake_lan_broadcast: bool,
    pub known_services: Vec<RegisteredService>,
    pub stats: OptimizerStatsSnapshot,
    #[serde(default)]
    pub admin_url: Option<String>,
}

impl Default for ClientStatusSnapshot {
    fn default() -> Self {
        Self {
            running: false,
            state: "idle".to_string(),
            server_addr: String::new(),
            transport: "quic".to_string(),
            listen_addr: "127.0.0.1:25565".to_string(),
            fake_lan_broadcast: true,
            known_services: Vec::new(),
            stats: OptimizerStatsSnapshot::default(),
            admin_url: None,
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

        // 4. Unknown host with multiple services -> None
        assert!(match_target_service(&services, Some("unknown.host.com")).is_none());
        assert!(match_target_service(&services, None).is_none());

        // 5. Single service -> defaults even if host is None or unknown
        let single = vec![services[0].clone()];
        let s = match_target_service(&single, None).unwrap();
        assert_eq!(s.name, "survival");

        let s = match_target_service(&single, Some("127.0.0.1:25565")).unwrap();
        assert_eq!(s.name, "survival");
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
            }),
            websocket: None,
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
            if waited > 3000 {
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
        assert!(controller.logs(100).await.is_empty());
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
            if waited > 3000 {
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
    async fn test_client_e2e_in_band_admin_bridge() {
        use crate::prism::tunnel::manager::Manager;
        use crate::prism::tunnel::server::{QuicServerOptions, Server, ServerOptions};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // 1. Mock local admin server on the server side
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

        // 2. Start tunnel server (with admin_addr = Some(admin_addr))
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

        // 3. Start client connecting to tunnel server
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
            })
            .unwrap(),
        );

        let c_clone = client.clone();
        let client_shutdown = shutdown_rx.clone();
        tokio::spawn(async move {
            let _ = c_clone.run(client_shutdown).await;
        });

        // Wait for client to connect and bridge to become available
        let mut admin_url = None;
        for _ in 0..60 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let st = client.status().await;
            if st.state == "connected" && st.admin_url.is_some() {
                admin_url = st.admin_url;
                break;
            }
        }
        let admin_url = admin_url.expect("client should be connected with admin_url");

        // 4. Request /health on the client's local admin_url
        let mut stream = tokio::net::TcpStream::connect(admin_url.strip_prefix("http://").unwrap())
            .await
            .expect("should connect to local admin bridge");

        stream
            .write_all(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        let resp = String::from_utf8_lossy(&buf[..n]);
        assert!(resp.contains("HTTP/1.1 200 OK"));
        assert!(resp.contains("{\"ok\":true}"));

        shutdown_tx.send(true).unwrap();
    }
}
