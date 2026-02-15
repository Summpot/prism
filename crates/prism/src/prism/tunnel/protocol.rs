use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MAGIC_REGISTER: &[u8; 4] = b"PRRG"; // Prism Reverse Register
const MAGIC_PROXY_TCP: &[u8; 4] = b"PRPX"; // Prism Reverse Proxy (TCP stream)
const MAGIC_PROXY_UDP: &[u8; 4] = b"PRPU"; // Prism Reverse Proxy (UDP datagram stream)
const PROTOCOL_V1: u8 = 1;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub services: Vec<RegisteredService>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

pub async fn write_proxy_stream_header<W: AsyncWrite + Unpin>(
    w: &mut W,
    kind: ProxyStreamKind,
    service: &str,
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
    write_mc_string(w, service).await?;
    Ok(())
}

pub async fn read_proxy_stream_header<R: AsyncRead + Unpin>(
    r: &mut R,
) -> Result<(ProxyStreamKind, String), ProtocolError> {
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

    let s = read_mc_string(r).await?;
    let s = s.trim().to_string();
    if s.is_empty() {
        return Err(ProtocolError::EmptyService);
    }
    Ok((kind, s))
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
                },
                RegisteredService {
                    name: "   ".into(),
                    proto: "tcp".into(),
                    local_addr: "x".into(),
                    route_only: false,
                    remote_addr: "".into(),
                    masquerade_host: "".into(),
                },
                RegisteredService {
                    name: "svc2".into(),
                    proto: "UDP".into(),
                    local_addr: " 127.0.0.1:19132 ".into(),
                    route_only: true,
                    remote_addr: "127.0.0.1:9999".into(),
                    masquerade_host: "svc2.internal".into(),
                },
            ],
        };

        let w = tokio::spawn(async move { write_register_request(&mut a, &req).await });
        let r = read_register_request(&mut b).await;
        w.await.unwrap().unwrap();

        let got = r.unwrap();
        assert_eq!(got.token, " t "); // token is not normalized by design

        assert_eq!(got.services.len(), 2);
        assert_eq!(got.services[0].name, "svc1");
        assert_eq!(got.services[0].proto, "tcp");
        assert_eq!(got.services[0].local_addr, "127.0.0.1:25565");
        assert_eq!(got.services[0].remote_addr, "127.0.0.1:0");
        assert_eq!(got.services[0].masquerade_host, "$1.edge.internal");

        assert_eq!(got.services[1].name, "svc2");
        assert_eq!(got.services[1].proto, "udp");
        assert!(got.services[1].route_only);
        // route_only clears remote_addr
        assert_eq!(got.services[1].remote_addr, "");
        assert_eq!(got.services[1].masquerade_host, "svc2.internal");
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
}
