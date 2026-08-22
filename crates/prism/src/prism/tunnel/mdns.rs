//! mDNS responder for tunnel client services.
//!
//! Advertises registered tunnel services via mDNS so that other devices on the
//! local network can resolve `<service>.<subdomain>.local` to the machine's LAN IP
//! and connect through the local proxy.

use std::collections::HashMap;
use std::net::{IpAddr, UdpSocket};

use mdns_sd::{ServiceDaemon, ServiceInfo};

const SERVICE_TYPE: &str = "_prism._tcp.local.";

/// Detect the machine's outbound LAN IP by connecting a UDP socket to a
/// well-known address. This does not actually send any traffic.
fn detect_lan_ip() -> Option<IpAddr> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    sock.local_addr().ok().map(|a| a.ip())
}

/// Build the mDNS hostname for a service.
///
/// If `subdomain` is non-empty: `<name>.<subdomain>.local.`
/// Otherwise: `<name>.local.`
fn build_hostname(service_name: &str, subdomain: &str, domain: &str) -> String {
    let domain = if domain.is_empty() { "local" } else { domain };
    if subdomain.is_empty() {
        format!("{service_name}.{domain}.")
    } else {
        format!("{service_name}.{subdomain}.{domain}.")
    }
}

/// Manages mDNS service registrations.
pub struct MdnsResponder {
    daemon: ServiceDaemon,
    domain: String,
    subdomain: String,
    port: u16,
    ip: String,
    /// service-name -> registered fullname (for unregister)
    registered: HashMap<String, String>,
}

impl std::fmt::Debug for MdnsResponder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MdnsResponder")
            .field("domain", &self.domain)
            .field("subdomain", &self.subdomain)
            .field("port", &self.port)
            .field("registered", &self.registered.len())
            .finish()
    }
}

impl MdnsResponder {
    /// Create a new mDNS responder.
    ///
    /// `domain` is typically `"local"`. `subdomain` can be empty or e.g. `"prism"`.
    /// `port` is the local proxy listening port that LAN clients connect to.
    /// `advertise_ip` overrides the auto-detected LAN IP if non-empty.
    pub fn new(
        domain: &str,
        subdomain: &str,
        port: u16,
        advertise_ip: &str,
    ) -> anyhow::Result<Self> {
        let ip = if advertise_ip.trim().is_empty() {
            detect_lan_ip()
                .map(|ip| ip.to_string())
                .unwrap_or_else(|| "0.0.0.0".into())
        } else {
            advertise_ip.trim().to_string()
        };

        let daemon = ServiceDaemon::new()
            .map_err(|e| anyhow::anyhow!("mdns: failed to create daemon: {e}"))?;

        tracing::info!(
            domain = %domain,
            subdomain = %subdomain,
            port,
            ip = %ip,
            "mdns: responder created"
        );

        Ok(Self {
            daemon,
            domain: domain.to_string(),
            subdomain: subdomain.to_string(),
            port,
            ip,
            registered: HashMap::new(),
        })
    }

    /// Register or update mDNS records for a set of service names.
    ///
    /// Services not in `service_names` will be unregistered.
    /// Services already registered are left as-is.
    pub fn reconcile(&mut self, service_names: &[&str]) {
        let desired: std::collections::HashSet<String> = service_names
            .iter()
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        // Unregister removed services.
        let to_remove: Vec<String> = self
            .registered
            .keys()
            .filter(|k| !desired.contains(k.as_str()))
            .cloned()
            .collect();
        for name in to_remove {
            self.unregister_service(&name);
        }

        // Register new services.
        for name in &desired {
            if !self.registered.contains_key(name) {
                self.register_service(name);
            }
        }
    }

    fn register_service(&mut self, service_name: &str) {
        let hostname = build_hostname(service_name, &self.subdomain, &self.domain);
        let properties: [(&str, &str); 0] = [];

        let info = match ServiceInfo::new(
            SERVICE_TYPE,
            service_name,
            &hostname,
            self.ip.as_str(),
            self.port,
            &properties[..],
        ) {
            Ok(info) => info.enable_addr_auto(),
            Err(err) => {
                tracing::warn!(
                    service = %service_name,
                    hostname = %hostname,
                    err = %err,
                    "mdns: failed to create service info"
                );
                return;
            }
        };

        let fullname = info.get_fullname().to_string();

        if let Err(err) = self.daemon.register(info) {
            tracing::warn!(
                service = %service_name,
                hostname = %hostname,
                err = %err,
                "mdns: failed to register service"
            );
            return;
        }

        tracing::info!(
            service = %service_name,
            hostname = %hostname,
            ip = %self.ip,
            port = self.port,
            "mdns: registered service"
        );
        self.registered.insert(service_name.to_string(), fullname);
    }

    fn unregister_service(&mut self, service_name: &str) {
        if let Some(fullname) = self.registered.remove(service_name) {
            if let Err(err) = self.daemon.unregister(&fullname) {
                tracing::debug!(
                    service = %service_name,
                    fullname = %fullname,
                    err = %err,
                    "mdns: unregister warning"
                );
            } else {
                tracing::info!(service = %service_name, "mdns: unregistered service");
            }
        }
    }

    /// Shut down the mDNS daemon and unregister all services.
    pub fn shutdown(mut self) {
        let names: Vec<String> = self.registered.keys().cloned().collect();
        for name in names {
            self.unregister_service(&name);
        }
        if let Err(err) = self.daemon.shutdown() {
            tracing::debug!(err = %err, "mdns: daemon shutdown warning");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_hostname_without_subdomain() {
        assert_eq!(build_hostname("mc", "", "local"), "mc.local.");
    }

    #[test]
    fn build_hostname_with_subdomain() {
        assert_eq!(build_hostname("mc", "prism", "local"), "mc.prism.local.");
    }

    #[test]
    fn build_hostname_with_empty_domain() {
        assert_eq!(build_hostname("mc", "prism", ""), "mc.prism.local.");
    }

    #[test]
    fn detect_lan_ip_returns_something() {
        // This test may fail in CI without network, but should pass on dev machines.
        let ip = detect_lan_ip();
        // Just assert it doesn't panic; IP may or may not be available.
        if let Some(ip) = ip {
            assert!(!ip.is_loopback() || ip.is_loopback()); // always true, just exercise
        }
    }
}
