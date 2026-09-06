use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MAGIC_REGISTER: &[u8; 4] = b"PRRG"; // Prism Reverse Register
const MAGIC_PROXY_TCP: &[u8; 4] = b"PRPX"; // Prism Reverse Proxy (TCP stream)
const MAGIC_PROXY_UDP: &[u8; 4] = b"PRPU"; // Prism Reverse Proxy (UDP datagram stream)
pub const MAGIC_SERVICE_CATALOG: &[u8; 4] = b"PRSC"; // Prism Reverse Service Catalog
pub const ADMIN_SERVICE_NAME: &str = "$admin";
const PROTOCOL_V1: u8 = 1;

pub const FLAG_RAW: u8 = 0x00;
pub const FLAG_OPTIMIZER: u8 = 0x01;

pub const MAX_REGISTER_JSON_BYTES: u32 = 1 << 20; // 1 MiB
pub const MAX_DATAGRAM_BYTES: u32 = 1 << 20; // 1 MiB

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("bad magic")]
    BadMagic,
    #[error("unsupported version")]
    BadVersion,
    #[error("payload too large: {0}")]
    PayloadTooLarge(u32),
    #[error("empty service")]
    EmptyService,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

fn default_client_type() -> String {
    "connector".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    #[serde(default = "default_client_type")]
    pub client_type: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub services: Vec<RegisteredService>,
}

impl Default for RegisterRequest {
    fn default() -> Self {
        Self {
            client_type: default_client_type(),
            token: String::new(),
            services: Vec::new(),
        }
    }
}

impl RegisterRequest {
    pub fn is_client(&self) -> bool {
        self.client_type.trim().eq_ignore_ascii_case("client")
    }

    pub fn is_connector(&self) -> bool {
        !self.is_client()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredService {
    pub name: String,
    #[serde(default)]
    pub proto: String, // tcp | udp
    #[serde(default)]
    pub local_addr: String,
    #[serde(default)]
    pub route_only: bool,
    #[serde(default)]
    pub remote_addr: String,
    /// Optional host label for rewrite middlewares when this service is dialed as an upstream
    /// (tunnel:<service>). This supports $1, $2... substitutions from route wildcard captures.
    #[serde(default)]
    pub masquerade_host: String,
    #[serde(default)]
    pub middleware: Option<String>,
    #[serde(default)]
    pub optimizer: Option<crate::prism::config::OptimizerConfig>,
}

impl RegisteredService {
    pub fn normalize(mut self) -> Option<Self> {
        self.name = self.name.trim().to_string();
        if self.name.is_empty() {
            return None;
        }
        self.proto = self.proto.trim().to_ascii_lowercase();
        if self.proto.is_empty() {
            self.proto = "tcp".into();
        }
        self.local_addr = self.local_addr.trim().to_string();
        self.remote_addr = self.remote_addr.trim().to_string();
        self.masquerade_host = self.masquerade_host.trim().to_ascii_lowercase();
        if self.route_only {
            self.remote_addr.clear();
        }
        self.middleware = self
            .middleware
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty());
        Some(self)
    }
}

pub async fn write_register_request<W: AsyncWrite + Unpin>(
    w: &mut W,
    req: &RegisterRequest,
) -> Result<(), ProtocolError> {
    w.write_all(MAGIC_REGISTER).await?;
    w.write_u8(PROTOCOL_V1).await?;

    let b = serde_json::to_vec(req)?;
    let n: u32 = b.len().try_into().unwrap_or(u32::MAX);
    w.write_u32(n).await?;
    w.write_all(&b).await?;
    w.flush().await?;
    Ok(())
}

pub async fn read_register_request<R: AsyncRead + Unpin>(
    r: &mut R,
) -> Result<RegisterRequest, ProtocolError> {
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic).await?;
    if &magic != MAGIC_REGISTER {
        return Err(ProtocolError::BadMagic);
    }

    let ver = r.read_u8().await?;
    if ver != PROTOCOL_V1 {
        return Err(ProtocolError::BadVersion);
    }

    let n = r.read_u32().await?;
    if n > MAX_REGISTER_JSON_BYTES {
        return Err(ProtocolError::PayloadTooLarge(n));
    }

    let mut buf = vec![0u8; n as usize];
    r.read_exact(&mut buf).await?;
    let mut req: RegisterRequest = serde_json::from_slice(&buf)?;

    let mut services = Vec::with_capacity(req.services.len());
    for s in req.services.drain(..) {
        if let Some(ns) = s.normalize() {
            services.push(ns);
        }
    }
    req.services = services;
    Ok(req)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyStreamKind {
    Tcp,
    Udp,
}

pub async fn write_proxy_stream_header_with_flags<W: AsyncWrite + Unpin>(
    w: &mut W,
    kind: ProxyStreamKind,
    service: &str,
    flags: u8,
) -> Result<(), ProtocolError> {
    let service = service.trim();
    if service.is_empty() {
        return Err(ProtocolError::EmptyService);
    }

    match kind {
        ProxyStreamKind::Tcp => w.write_all(MAGIC_PROXY_TCP).await?,
        ProxyStreamKind::Udp => w.write_all(MAGIC_PROXY_UDP).await?,
    }
    w.write_u8(PROTOCOL_V1).await?;
    w.write_u8(flags).await?;
    write_mc_string(w, service).await?;
    Ok(())
}

pub async fn write_proxy_stream_header<W: AsyncWrite + Unpin>(
    w: &mut W,
    kind: ProxyStreamKind,
    service: &str,
) -> Result<(), ProtocolError> {
    write_proxy_stream_header_with_flags(w, kind, service, FLAG_RAW).await
}

pub async fn read_proxy_stream_header_with_flags<R: AsyncRead + Unpin>(
    r: &mut R,
) -> Result<(ProxyStreamKind, String, u8), ProtocolError> {
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic).await?;

    let kind = if &magic == MAGIC_PROXY_TCP {
        ProxyStreamKind::Tcp
    } else if &magic == MAGIC_PROXY_UDP {
        ProxyStreamKind::Udp
    } else {
        return Err(ProtocolError::BadMagic);
    };

    let ver = r.read_u8().await?;
    if ver != PROTOCOL_V1 {
        return Err(ProtocolError::BadVersion);
    }

    let flags = r.read_u8().await?;

    let s = read_mc_string(r).await?;
    let s = s.trim().to_string();
    if s.is_empty() {
        return Err(ProtocolError::EmptyService);
    }
    Ok((kind, s, flags))
}

pub async fn read_proxy_stream_header<R: AsyncRead + Unpin>(
    r: &mut R,
) -> Result<(ProxyStreamKind, String), ProtocolError> {
    let (kind, s, _flags) = read_proxy_stream_header_with_flags(r).await?;
    Ok((kind, s))
}

pub async fn write_service_catalog<W: AsyncWrite + Unpin>(
    w: &mut W,
    services: &[RegisteredService],
) -> Result<(), ProtocolError> {
    w.write_all(MAGIC_SERVICE_CATALOG).await?;
    w.write_u8(PROTOCOL_V1).await?;

    let b = serde_json::to_vec(services)?;
    let n: u32 = b.len().try_into().unwrap_or(u32::MAX);
    w.write_u32(n).await?;
    w.write_all(&b).await?;
    w.flush().await?;
    Ok(())
}

pub async fn read_service_catalog<R: AsyncRead + Unpin>(
    r: &mut R,
) -> Result<Vec<RegisteredService>, ProtocolError> {
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic).await?;
    if &magic != MAGIC_SERVICE_CATALOG {
        return Err(ProtocolError::BadMagic);
    }

    let ver = r.read_u8().await?;
    if ver != PROTOCOL_V1 {
        return Err(ProtocolError::BadVersion);
    }

    let n = r.read_u32().await?;
    if n > MAX_REGISTER_JSON_BYTES {
        return Err(ProtocolError::PayloadTooLarge(n));
    }

    let mut buf = vec![0u8; n as usize];
    r.read_exact(&mut buf).await?;
    let mut svcs: Vec<RegisteredService> = serde_json::from_slice(&buf)?;
    let mut normalized = Vec::with_capacity(svcs.len());
    for s in svcs.drain(..) {
        if let Some(ns) = s.normalize() {
            normalized.push(ns);
        }
    }
    Ok(normalized)
}

async fn write_mc_string<W: AsyncWrite + Unpin>(w: &mut W, s: &str) -> Result<(), ProtocolError> {
    let b = s.as_bytes();
    write_varint(w, b.len() as i32).await?;
    w.write_all(b).await?;
    Ok(())
}

async fn read_mc_string<R: AsyncRead + Unpin>(
    r: &mut R,
) -> Result<Cow<'static, str>, ProtocolError> {
    let len = read_varint(r).await?;
    if len < 0 {
        return Err(ProtocolError::BadMagic);
    }
    let len: usize = len as usize;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).await?;
    Ok(Cow::Owned(String::from_utf8_lossy(&buf).into_owned()))
}

async fn write_varint<W: AsyncWrite + Unpin>(w: &mut W, mut v: i32) -> Result<(), ProtocolError> {
    loop {
        let mut temp = (v & 0x7f) as u8;
        v = ((v as u32) >> 7) as i32;
        if v != 0 {
            temp |= 0x80;
        }
        w.write_u8(temp).await?;
        if v == 0 {
            break;
        }
    }
    Ok(())
}

async fn read_varint<R: AsyncRead + Unpin>(r: &mut R) -> Result<i32, ProtocolError> {
    let mut num_read = 0;
    let mut result: i32 = 0;
    loop {
        let read = r.read_u8().await?;
        let value = (read & 0x7F) as i32;
        result |= value << (7 * num_read);

        num_read += 1;
        if num_read > 5 {
            return Err(ProtocolError::BadMagic);
        }

        if (read & 0x80) == 0 {
            break;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_roundtrip_normalizes_services() {
        let (mut a, mut b) = tokio::io::duplex((MAX_REGISTER_JSON_BYTES as usize).max(1024));

        let req = RegisterRequest {
            token: " t ".into(),
            services: vec![
                RegisteredService {
                    name: "  svc1 ".into(),
                    proto: "".into(),
                    local_addr: " 127.0.0.1:25565 ".into(),
                    route_only: false,
                    remote_addr: " 127.0.0.1:0 ".into(),
                    masquerade_host: "  $1.edge.internal  ".into(),
                    middleware: Some("  minecraft.wat  ".into()),
                    optimizer: Some(crate::prism::config::OptimizerConfig {
                        enabled: true,
                        flush_interval_ms: Some(20),
                        zstd_window_log: Some(23),
                        zstd_level: Some(3),
                    }),
                },
                RegisteredService {
                    name: "   ".into(),
                    proto: "tcp".into(),
                    local_addr: "x".into(),
                    route_only: false,
                    remote_addr: "".into(),
                    masquerade_host: "".into(),
                    middleware: None,
                    optimizer: None,
                },
                RegisteredService {
                    name: "svc2".into(),
                    proto: "UDP".into(),
                    local_addr: " 127.0.0.1:19132 ".into(),
                    route_only: true,
                    remote_addr: "127.0.0.1:9999".into(),
                    masquerade_host: "svc2.internal".into(),
                    middleware: Some("".into()),
                    optimizer: None,
                },
            ],
            ..Default::default()
        };

        let w = tokio::spawn(async move { write_register_request(&mut a, &req).await });
        let r = read_register_request(&mut b).await;
        w.await.unwrap().unwrap();

        let got = r.unwrap();
        assert_eq!(got.token, " t "); // token is not normalized by design
        assert_eq!(got.client_type, "connector");
        assert!(got.is_connector());
        assert!(!got.is_client());

        assert_eq!(got.services.len(), 2);
        assert_eq!(got.services[0].name, "svc1");
        assert_eq!(got.services[0].proto, "tcp");
        assert_eq!(got.services[0].local_addr, "127.0.0.1:25565");
        assert_eq!(got.services[0].remote_addr, "127.0.0.1:0");
        assert_eq!(got.services[0].masquerade_host, "$1.edge.internal");
        assert_eq!(got.services[0].middleware.as_deref(), Some("minecraft.wat"));
        assert!(got.services[0].optimizer.as_ref().unwrap().enabled);

        assert_eq!(got.services[1].name, "svc2");
        assert_eq!(got.services[1].proto, "udp");
        assert!(got.services[1].route_only);
        // route_only clears remote_addr
        assert_eq!(got.services[1].remote_addr, "");
        assert_eq!(got.services[1].masquerade_host, "svc2.internal");
        assert_eq!(got.services[1].middleware, None);
    }

    #[tokio::test]
    async fn register_rejects_too_large_length_without_reading_payload() {
        let (mut a, mut b) = tokio::io::duplex(128);

        tokio::spawn(async move {
            a.write_all(MAGIC_REGISTER).await.unwrap();
            a.write_u8(PROTOCOL_V1).await.unwrap();
            a.write_u32(MAX_REGISTER_JSON_BYTES + 1).await.unwrap();
            // no payload needed
        });

        let err = read_register_request(&mut b).await.unwrap_err();
        match err {
            ProtocolError::PayloadTooLarge(n) => assert!(n > MAX_REGISTER_JSON_BYTES),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn proxy_header_roundtrip_trims_service() {
        let (mut a, mut b) = tokio::io::duplex(128);
        tokio::spawn(async move {
            write_proxy_stream_header(&mut a, ProxyStreamKind::Tcp, "  svc  ").await
        });

        let (kind, svc) = read_proxy_stream_header(&mut b).await.unwrap();
        assert_eq!(kind, ProxyStreamKind::Tcp);
        assert_eq!(svc, "svc");
    }

    #[tokio::test]
    async fn proxy_header_with_flags_roundtrip() {
        let (mut a, mut b) = tokio::io::duplex(128);
        tokio::spawn(async move {
            write_proxy_stream_header_with_flags(
                &mut a,
                ProxyStreamKind::Tcp,
                "  game-svc  ",
                FLAG_OPTIMIZER,
            )
            .await
        });

        let (kind, svc, flags) = read_proxy_stream_header_with_flags(&mut b).await.unwrap();
        assert_eq!(kind, ProxyStreamKind::Tcp);
        assert_eq!(svc, "game-svc");
        assert_eq!(flags, FLAG_OPTIMIZER);
    }

    #[tokio::test]
    async fn register_request_client_type_detection() {
        let req = RegisterRequest {
            client_type: "client".into(),
            token: "tok".into(),
            services: vec![],
        };
        assert!(req.is_client());
        assert!(!req.is_connector());

        let json = r#"{"token":"abc"}"#;
        let parsed: RegisterRequest = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.client_type, "connector");
        assert!(parsed.is_connector());
    }

    #[tokio::test]
    async fn service_catalog_roundtrip() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        let services = vec![RegisteredService {
            name: "web".into(),
            proto: "tcp".into(),
            local_addr: "127.0.0.1:8080".into(),
            route_only: false,
            remote_addr: "".into(),
            masquerade_host: "".into(),
            middleware: Some("minecraft.wat".into()),
            optimizer: Some(crate::prism::config::OptimizerConfig {
                enabled: true,
                flush_interval_ms: Some(20),
                zstd_window_log: Some(23),
                zstd_level: Some(3),
            }),
        }];

        let svcs_clone = services.clone();
        tokio::spawn(async move {
            write_service_catalog(&mut a, &svcs_clone).await.unwrap();
        });

        let received = read_service_catalog(&mut b).await.unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].name, "web");
        assert_eq!(received[0].middleware.as_deref(), Some("minecraft.wat"));
        assert!(received[0].optimizer.as_ref().unwrap().enabled);
    }
}
