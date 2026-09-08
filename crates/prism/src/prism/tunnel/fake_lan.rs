//! Generic UDP service discovery broadcaster (supporting Minecraft LAN discovery, SSDP, custom UDP broadcasts).
//!
//! Discovery targets and payload framing are decoupled into WASM protocol drivers
//! (e.g. `middlewares/minecraft.wat`), with fallback dynamic templating.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

/// Detect the machine's outbound LAN IPv4 address by connecting a UDP socket to a
/// well-known address. This does not actually send any traffic.
pub fn detect_lan_ipv4() -> Option<std::net::Ipv4Addr> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    match sock.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(v4) => Some(v4),
        std::net::IpAddr::V6(_) => None,
    }
}

/// Computes the list of destination addresses to send LAN broadcast packets to.
/// Automatically includes subnet broadcast `{subnet}.255:{port}` if any target is a
/// multicast or broadcast address and a local LAN IPv4 address is detected.
pub fn resolve_broadcast_targets(target_addr: &str) -> Vec<String> {
    let raw_targets: Vec<&str> = target_addr
        .split([',', ';'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    let mut targets = Vec::new();
    let lan_ip = detect_lan_ipv4();

    for t in raw_targets {
        if !targets.iter().any(|existing: &String| existing == t) {
            targets.push(t.to_string());
        }

        // Extract port and check if target is multicast/broadcast
        if let Ok(addr) = t.parse::<std::net::SocketAddr>() {
            let ip = addr.ip();
            let port = addr.port();
            let is_multi_or_bcast = match ip {
                std::net::IpAddr::V4(v4) => v4.is_multicast() || v4.is_broadcast(),
                std::net::IpAddr::V6(v6) => v6.is_multicast(),
            };

            if is_multi_or_bcast {
                if let Some(lip) = lan_ip {
                    let octets = lip.octets();
                    let subnet_bcast =
                        format!("{}.{}.{}.255:{port}", octets[0], octets[1], octets[2]);
                    if !targets.contains(&subnet_bcast) {
                        targets.push(subnet_bcast);
                    }
                }
            }
        }
    }

    if targets.is_empty() && !target_addr.is_empty() {
        targets.push(target_addr.to_string());
    }

    targets
}

/// Default interval between periodic broadcast packets (1.5 seconds).
pub const DEFAULT_BROADCAST_INTERVAL: Duration = Duration::from_millis(1500);

/// Render discovery payload string from template with placeholder substitutions.
pub fn render_payload(template: &str, motd_prefix: &str, service_name: &str, port: u16) -> String {
    template
        .replace("{motd_prefix}", motd_prefix)
        .replace("{prefix}", motd_prefix)
        .replace("{service_name}", service_name)
        .replace("{name}", service_name)
        .replace("{service}", service_name)
        .replace("{port}", &port.to_string())
}

/// An active advertised service for UDP service discovery broadcast.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdvertisedService {
    pub name: String,
    pub port: u16,
    pub motd_prefix: String,
    pub template: Option<String>,
    payload: String,
}

impl AdvertisedService {
    pub fn new(name: impl Into<String>, port: u16, motd_prefix: impl Into<String>) -> Self {
        let name = name.into();
        let motd_prefix = motd_prefix.into();
        let payload =
            crate::prism::middleware::build_default_discovery_payload(&name, port, &motd_prefix)
                .unwrap_or_else(|| format!("{motd_prefix}{name}:{port}"));
        Self {
            name,
            port,
            motd_prefix,
            template: None,
            payload,
        }
    }

    pub fn with_template(
        name: impl Into<String>,
        port: u16,
        motd_prefix: impl Into<String>,
        template: impl Into<String>,
    ) -> Self {
        let name = name.into();
        let motd_prefix = motd_prefix.into();
        let template = template.into();
        let payload = render_payload(&template, &motd_prefix, &name, port);
        Self {
            name,
            port,
            motd_prefix,
            template: Some(template),
            payload,
        }
    }

    /// Formats the service as a discovery broadcast payload.
    pub fn to_payload(&self) -> &str {
        &self.payload
    }
}

/// Formats a discovery broadcast message string using the default protocol driver.
pub fn format_payload(motd_prefix: &str, service_name: &str, port: u16) -> String {
    crate::prism::middleware::build_default_discovery_payload(service_name, port, motd_prefix)
        .unwrap_or_else(|| format!("{motd_prefix}{service_name}:{port}"))
}

/// Periodic UDP multicast/broadcast service advertiser.
///
/// Broadcasts service discovery packets (e.g. to `224.0.2.60:4445` for Minecraft LAN, or
/// custom discovery endpoints) so clients can automatically discover services on local networks.
#[allow(dead_code)]
pub type UdpDiscoveryBroadcaster = FakeLanBroadcaster;
#[derive(Debug, Clone)]
pub struct FakeLanBroadcaster {
    services: Arc<RwLock<Vec<AdvertisedService>>>,
    target_addr: String,
    interval: Duration,
}

impl Default for FakeLanBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeLanBroadcaster {
    /// Creates a new broadcaster using default discovery targets from the protocol driver.
    pub fn new() -> Self {
        let targets = crate::prism::middleware::get_default_discovery_targets();
        let target_addr = targets.join(",");
        Self::with_target(target_addr, DEFAULT_BROADCAST_INTERVAL)
    }

    /// Creates a new broadcaster with custom target address and interval (useful for tests).
    pub fn with_target(target_addr: impl Into<String>, interval: Duration) -> Self {
        Self {
            services: Arc::new(RwLock::new(Vec::new())),
            target_addr: target_addr.into(),
            interval,
        }
    }

    /// Returns a snapshot of current advertised services.
    pub async fn services(&self) -> Vec<AdvertisedService> {
        self.services.read().await.clone()
    }

    /// Dynamically sets/replaces all active advertised services.
    pub async fn set_services(&self, services: Vec<AdvertisedService>) {
        let mut guard = self.services.write().await;
        *guard = services;
    }

    /// Adds a service to the active advertisement list if not already present.
    pub async fn add_service(&self, service: AdvertisedService) {
        let mut guard = self.services.write().await;
        if !guard
            .iter()
            .any(|s| s.name == service.name && s.port == service.port)
        {
            guard.push(service);
        }
    }

    /// Removes an advertised service by name.
    pub async fn remove_service(&self, name: &str) {
        let mut guard = self.services.write().await;
        guard.retain(|s| s.name != name);
    }

    /// Clears all active advertised services.
    pub async fn clear(&self) {
        let mut guard = self.services.write().await;
        guard.clear();
    }

    /// Runs the periodic broadcast loop until `shutdown` receives `true`.
    pub async fn run(
        &self,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        if *shutdown.borrow() {
            return Ok(());
        }

        let socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
        let _ = socket.set_broadcast(true);
        let _ = socket.set_multicast_loop_v4(true);
        let _ = socket.set_multicast_ttl_v4(2);

        let lan_ip = detect_lan_ipv4();
        // Also create a dedicated LAN-bound socket if an outbound LAN IP is detected.
        // Binding directly to the LAN IP forces Windows/Linux to route both multicast and broadcast
        // out of the physical network adapter rather than virtual adapters (Hyper-V / WSL).
        let lan_socket = if let Some(ip) = lan_ip {
            match tokio::net::UdpSocket::bind(std::net::SocketAddr::from((ip, 0))).await {
                Ok(s) => {
                    let _ = s.set_broadcast(true);
                    let _ = s.set_multicast_loop_v4(true);
                    let _ = s.set_multicast_ttl_v4(2);
                    Some(s)
                }
                Err(_) => None,
            }
        } else {
            None
        };

        let targets = resolve_broadcast_targets(&self.target_addr);

        tracing::info!(
            target = %self.target_addr,
            targets = ?targets,
            lan_ip = ?lan_ip,
            interval = %humantime::format_duration(self.interval),
            "fake_lan: broadcaster started"
        );

        let mut ticker = tokio::time::interval(self.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::info!("fake_lan: broadcaster shutting down");
                        break;
                    }
                }
                _ = ticker.tick() => {
                    let active = self.services.read().await.clone();
                    for svc in active {
                        let payload = svc.to_payload();
                        let bytes = payload.as_bytes();
                        for target in &targets {
                            let sock = if target.starts_with("127.") {
                                &socket
                            } else if let Some(ref ls) = lan_socket {
                                ls
                            } else {
                                &socket
                            };
                            if let Err(err) = sock.send_to(bytes, target).await {
                                tracing::trace!(
                                    err = %err,
                                    target = %target,
                                    service = %svc.name,
                                    "fake_lan: send_to skipped target"
                                );
                            } else {
                                tracing::trace!(
                                    target = %target,
                                    service = %svc.name,
                                    payload = %payload,
                                    "fake_lan: broadcast packet sent"
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_payload_matches_spec() {
        let payload = format_payload("[Prism] ", "生存服", 25565);
        assert_eq!(payload, "[MOTD][Prism] 生存服[/MOTD][AD]25565[/AD]");

        let svc = AdvertisedService::new("Minecraft Server", 25566, "[Tunnel] ");
        assert_eq!(
            svc.to_payload(),
            "[MOTD][Tunnel] Minecraft Server[/MOTD][AD]25566[/AD]"
        );
    }

    #[test]
    fn test_custom_discovery_template() {
        let svc = AdvertisedService::with_template(
            "valheim-server",
            2456,
            "prefix-",
            "DISCOVER:{name}:{port}:{prefix}",
        );
        assert_eq!(svc.to_payload(), "DISCOVER:valheim-server:2456:prefix-");
    }

    #[tokio::test]
    async fn test_dynamic_service_updates() {
        let broadcaster = FakeLanBroadcaster::new();
        assert!(broadcaster.services().await.is_empty());

        // Add services
        broadcaster
            .add_service(AdvertisedService::new("svc1", 25565, "[Prism] "))
            .await;
        broadcaster
            .add_service(AdvertisedService::new("svc2", 25566, "[Prism] "))
            .await;
        assert_eq!(broadcaster.services().await.len(), 2);

        // Deduplication on add
        broadcaster
            .add_service(AdvertisedService::new("svc1", 25565, "[Prism] "))
            .await;
        assert_eq!(broadcaster.services().await.len(), 2);

        // Remove by name
        broadcaster.remove_service("svc1").await;
        let svcs = broadcaster.services().await;
        assert_eq!(svcs.len(), 1);
        assert_eq!(svcs[0].name, "svc2");

        // Set services replaces
        broadcaster
            .set_services(vec![AdvertisedService::new("svc3", 25567, "")])
            .await;
        let svcs = broadcaster.services().await;
        assert_eq!(svcs.len(), 1);
        assert_eq!(svcs[0].name, "svc3");

        // Clear
        broadcaster.clear().await;
        assert!(broadcaster.services().await.is_empty());
    }

    #[tokio::test]
    async fn test_broadcaster_send_and_receive_and_shutdown() {
        // Bind a local receiver socket to test actual UDP delivery
        let receiver = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let receiver_addr = receiver.local_addr().unwrap().to_string();

        let broadcaster = FakeLanBroadcaster::with_target(receiver_addr, Duration::from_millis(50));
        broadcaster
            .add_service(AdvertisedService::new("测试服", 25565, "[Prism] "))
            .await;

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let b_clone = broadcaster.clone();
        let handle = tokio::spawn(async move { b_clone.run(shutdown_rx).await });

        let mut buf = vec![0u8; 1024];
        let n = tokio::time::timeout(Duration::from_secs(3), receiver.recv(&mut buf))
            .await
            .expect("should receive packet within 3 seconds")
            .expect("receive should succeed");

        let msg = String::from_utf8_lossy(&buf[..n]);
        assert_eq!(msg, "[MOTD][Prism] 测试服[/MOTD][AD]25565[/AD]");

        // Signal graceful shutdown
        shutdown_tx.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("should terminate gracefully")
            .unwrap()
            .unwrap();
    }

    #[test]
    fn test_resolve_broadcast_targets() {
        let custom = resolve_broadcast_targets("192.168.1.50:4445");
        assert_eq!(custom, vec!["192.168.1.50:4445"]);

        let defaults =
            resolve_broadcast_targets("224.0.2.60:4445,255.255.255.255:4445,127.0.0.1:4445");
        assert!(defaults.contains(&"224.0.2.60:4445".to_string()));
        assert!(defaults.contains(&"255.255.255.255:4445".to_string()));
        assert!(defaults.contains(&"127.0.0.1:4445".to_string()));
    }
}
