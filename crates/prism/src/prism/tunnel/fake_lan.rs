//! Fake LAN multicast broadcaster for Minecraft LAN discovery.
//!
//! Specifications:
//! - Multicast IP: `224.0.2.60`, Port: `4445` (UDP)
//! - Payload format: `[MOTD]{motd_prefix}{service_name}[/MOTD][AD]{port}[/AD]`
//!   Example: `[MOTD][Prism] 生存服[/MOTD][AD]25565[/AD]`

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

/// Standard multicast address used by Minecraft LAN server discovery.
pub const MINECRAFT_LAN_MULTICAST_ADDR: &str = "224.0.2.60:4445";

/// Standard global broadcast address used by Minecraft LAN server discovery.
pub const MINECRAFT_LAN_BROADCAST_ADDR: &str = "255.255.255.255:4445";

/// Standard loopback address for local Minecraft client discovery.
pub const MINECRAFT_LAN_LOOPBACK_ADDR: &str = "127.0.0.1:4445";

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
pub fn resolve_broadcast_targets(target_addr: &str) -> Vec<String> {
    if target_addr != MINECRAFT_LAN_MULTICAST_ADDR {
        return vec![target_addr.to_string()];
    }

    let mut targets = vec![
        MINECRAFT_LAN_MULTICAST_ADDR.to_string(),
        MINECRAFT_LAN_BROADCAST_ADDR.to_string(),
        MINECRAFT_LAN_LOOPBACK_ADDR.to_string(),
    ];

    if let Some(ip) = detect_lan_ipv4() {
        let octets = ip.octets();
        let subnet_bcast = format!("{}.{}.{}.255:4445", octets[0], octets[1], octets[2]);
        if !targets.contains(&subnet_bcast) {
            targets.push(subnet_bcast);
        }
    }

    targets
}

/// Default interval between periodic broadcast packets (1.5 seconds).
pub const DEFAULT_BROADCAST_INTERVAL: Duration = Duration::from_millis(1500);

/// An active advertised service for Fake LAN broadcast.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdvertisedService {
    pub name: String,
    pub port: u16,
    pub motd_prefix: String,
}

impl AdvertisedService {
    pub fn new(name: impl Into<String>, port: u16, motd_prefix: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            port,
            motd_prefix: motd_prefix.into(),
        }
    }

    /// Formats the service as a standard Minecraft LAN broadcast payload.
    pub fn to_payload(&self) -> String {
        format_payload(&self.motd_prefix, &self.name, self.port)
    }
}

/// Formats a Minecraft LAN broadcast message string:
/// `[MOTD]{motd_prefix}{service_name}[/MOTD][AD]{port}[/AD]`
pub fn format_payload(motd_prefix: &str, service_name: &str, port: u16) -> String {
    format!(
        "[MOTD]{}{}[/MOTD][AD]{}[/AD]",
        motd_prefix, service_name, port
    )
}

/// Periodic Minecraft LAN UDP multicast broadcaster.
///
/// Broadcasts Minecraft LAN discovery packets to `224.0.2.60:4445` so players
/// see available tunnel services in their in-game multiplayer LAN list without
/// manually entering server addresses.
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
    /// Creates a new broadcaster pointing to `224.0.2.60:4445` with 1.5s interval.
    pub fn new() -> Self {
        Self::with_target(MINECRAFT_LAN_MULTICAST_ADDR, DEFAULT_BROADCAST_INTERVAL)
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

        let defaults = resolve_broadcast_targets(MINECRAFT_LAN_MULTICAST_ADDR);
        assert!(defaults.contains(&MINECRAFT_LAN_MULTICAST_ADDR.to_string()));
        assert!(defaults.contains(&MINECRAFT_LAN_BROADCAST_ADDR.to_string()));
        assert!(defaults.contains(&MINECRAFT_LAN_LOOPBACK_ADDR.to_string()));
    }
}
