//! Local LAN proxy for tunnel client mDNS.
//!
//! When mDNS is enabled, this module opens a TCP listener on the configured
//! address (typically `0.0.0.0:<port>`) so that other devices on the local
//! network can connect. Incoming connections are routed to the correct local
//! service using the same middleware-based hostname extraction as the remote
//! server.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use tokio::net::{TcpListener, TcpStream};

use crate::prism::middleware::{MiddlewareChain, MiddlewareError, SharedMiddlewareChain};
use crate::prism::net;

/// Configuration for the local proxy.
#[derive(Debug, Clone)]
pub struct LocalProxyConfig {
    /// Address to bind on (e.g. `":25565"` -> `"0.0.0.0:25565"`).
    pub listen_addr: String,
    /// Max bytes to buffer for middleware hostname extraction.
    pub max_header_bytes: usize,
    /// Timeout for the initial handshake/hostname extraction.
    pub handshake_timeout: Duration,
    /// Domain suffix used in mDNS hostnames (e.g. `"local"`).
    pub domain: String,
    /// Optional subdomain label (e.g. `"prism"`, so hostnames are `<name>.prism.local`).
    pub subdomain: String,
}

/// A mapping from service name to local address.
pub type ServiceMap = Arc<HashMap<String, String>>;

pub struct LocalProxy {
    config: LocalProxyConfig,
    services: ServiceMap,
    middleware: SharedMiddlewareChain,
}

impl std::fmt::Debug for LocalProxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalProxy")
            .field("listen_addr", &self.config.listen_addr)
            .field("services", &self.services.len())
            .finish()
    }
}

impl LocalProxy {
    pub fn new(
        config: LocalProxyConfig,
        services: ServiceMap,
        middleware: SharedMiddlewareChain,
    ) -> Self {
        Self {
            config,
            services,
            middleware,
        }
    }

    pub async fn run(
        &self,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let bind_addr = net::normalize_bind_addr(&self.config.listen_addr);
        let ln = TcpListener::bind(bind_addr.as_ref())
            .await
            .with_context(|| format!("mdns: local proxy bind {}", self.config.listen_addr))?;

        let local = ln.local_addr().ok();
        tracing::info!(
            listen_addr = %self.config.listen_addr,
            local = ?local,
            services = self.services.len(),
            "mdns: local proxy listening"
        );

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
                res = ln.accept() => {
                    let (conn, peer) = res?;
                    let config = self.config.clone();
                    let services = self.services.clone();
                    let middleware = self.middleware.clone();
                    tokio::spawn(async move {
                        if let Err(err) = handle_conn(conn, &config, &services, &*middleware).await {
                            tracing::debug!(
                                peer = %peer,
                                err = %err,
                                "mdns: local proxy conn ended"
                            );
                        }
                    });
                }
            }
        }

        Ok(())
    }
}

async fn handle_conn(
    mut conn: TcpStream,
    config: &LocalProxyConfig,
    services: &HashMap<String, String>,
    middleware: &dyn MiddlewareChain,
) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Buffer initial bytes for hostname extraction.
    let mut prelude = Vec::with_capacity(config.max_header_bytes.min(4096));
    let mut buf = [0u8; 4096];

    let deadline = tokio::time::Instant::now() + config.handshake_timeout;

    let (host, prelude_override) = loop {
        if tokio::time::Instant::now() > deadline {
            anyhow::bail!("mdns: handshake timeout");
        }

        let remaining = config.max_header_bytes.saturating_sub(prelude.len());
        if remaining == 0 {
            anyhow::bail!("mdns: max header bytes exceeded without host match");
        }

        let to_read = remaining.min(buf.len());
        let n = tokio::time::timeout_at(deadline, conn.read(&mut buf[..to_read]))
            .await
            .context("mdns: handshake read timeout")?
            .context("mdns: handshake read")?;

        if n == 0 {
            anyhow::bail!("mdns: connection closed during handshake");
        }
        prelude.extend_from_slice(&buf[..n]);

        match middleware.parse(&prelude) {
            Ok((host, override_bytes)) => break (host, override_bytes),
            Err(MiddlewareError::NeedMoreData) => continue,
            Err(MiddlewareError::NoMatch) => {
                anyhow::bail!("mdns: middleware no match");
            }
            Err(MiddlewareError::Fatal(err)) => {
                anyhow::bail!("mdns: middleware fatal: {err}");
            }
        }
    };

    // Extract service name from the parsed hostname.
    let service_name = extract_service_name(&host, &config.subdomain, &config.domain);
    if service_name.is_empty() {
        anyhow::bail!("mdns: could not extract service name from host '{host}'");
    }

    let local_addr = services.get(&service_name).cloned().ok_or_else(|| {
        anyhow::anyhow!("mdns: unknown service '{service_name}' from host '{host}'")
    })?;

    tracing::info!(
        host = %host,
        service = %service_name,
        local_addr = %local_addr,
        "mdns: local proxy routing"
    );

    // Connect to local service.
    let mut upstream = TcpStream::connect(&local_addr)
        .await
        .with_context(|| format!("mdns: connect to local service {local_addr}"))?;

    // Send the prelude (possibly rewritten) to the upstream.
    let to_send = prelude_override.as_deref().unwrap_or(&prelude);
    upstream.write_all(to_send).await?;

    // Bidirectional copy.
    let _ = tokio::io::copy_bidirectional(&mut conn, &mut upstream).await;

    Ok(())
}

/// Extract the service name from a hostname by stripping the domain and subdomain suffixes.
///
/// Examples:
/// - `"home-mc.prism.local"` with subdomain `"prism"` and domain `"local"` -> `"home-mc"`
/// - `"home-mc.local"` with subdomain `""` and domain `"local"` -> `"home-mc"`
/// - `"home-mc.prism.local:25565"` -> `"home-mc"` (port is stripped by normalize_routing_host)
fn extract_service_name(host: &str, subdomain: &str, domain: &str) -> String {
    // Normalize: lowercase, strip port.
    let h = crate::prism::router::normalize_routing_host(host);
    if h.is_empty() {
        return String::new();
    }

    // Build the suffix to strip.
    let suffix = if subdomain.is_empty() {
        format!(".{domain}")
    } else {
        format!(".{subdomain}.{domain}")
    };

    if let Some(name) = h.strip_suffix(&suffix) {
        name.to_string()
    } else {
        // Fallback: just use the first label.
        h.split('.').next().unwrap_or("").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_name_with_subdomain() {
        assert_eq!(
            extract_service_name("home-mc.prism.local", "prism", "local"),
            "home-mc"
        );
    }

    #[test]
    fn extract_name_without_subdomain() {
        assert_eq!(
            extract_service_name("home-mc.local", "", "local"),
            "home-mc"
        );
    }

    #[test]
    fn extract_name_with_port() {
        assert_eq!(
            extract_service_name("home-mc.prism.local:25565", "prism", "local"),
            "home-mc"
        );
    }

    #[test]
    fn extract_name_no_match_fallback() {
        assert_eq!(
            extract_service_name("something.other.tld", "prism", "local"),
            "something"
        );
    }

    #[test]
    fn extract_name_empty() {
        assert_eq!(extract_service_name("", "prism", "local"), "");
    }

    struct MockChain {
        host: String,
    }

    impl MiddlewareChain for MockChain {
        fn name(&self) -> &str {
            "mock"
        }

        fn parse(&self, _prelude: &[u8]) -> Result<(String, Option<Vec<u8>>), MiddlewareError> {
            Ok((self.host.clone(), None))
        }

        fn rewrite(&self, _prelude: &[u8], _selected_upstream: &str) -> Option<Vec<u8>> {
            None
        }
    }

    #[tokio::test]
    async fn local_proxy_routes_connection_to_backend() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // 1. Backend listener
        let backend = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_addr = backend.local_addr().unwrap().to_string();

        let (backend_done_tx, backend_done_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (mut stream, _) = backend.accept().await.unwrap();
            let mut buf = [0u8; 12];
            stream.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ping payload");
            stream.write_all(b"pong response").await.unwrap();
            let _ = backend_done_tx.send(());
        });

        // 2. Local Proxy
        let proxy_ln = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_ln.local_addr().unwrap().to_string();
        drop(proxy_ln); // free port

        let mut map = HashMap::new();
        map.insert("mysvc".to_string(), backend_addr);

        let proxy = LocalProxy::new(
            LocalProxyConfig {
                listen_addr: proxy_addr.clone(),
                max_header_bytes: 4096,
                handshake_timeout: Duration::from_secs(3),
                domain: "local".into(),
                subdomain: "prism".into(),
            },
            Arc::new(map),
            Arc::new(MockChain {
                host: "mysvc.prism.local".into(),
            }),
        );

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let proxy_task = tokio::spawn(async move {
            let _ = proxy.run(shutdown_rx).await;
        });

        // Wait briefly for proxy listener to be ready
        tokio::time::sleep(Duration::from_millis(50)).await;

        // 3. Connect client
        let mut client = TcpStream::connect(&proxy_addr).await.unwrap();
        client.write_all(b"ping payload").await.unwrap();
        let mut resp = [0u8; 13];
        client.read_exact(&mut resp).await.unwrap();
        assert_eq!(&resp, b"pong response");

        backend_done_rx.await.unwrap();
        let _ = shutdown_tx.send(true);
        let _ = proxy_task.await;
    }
}
