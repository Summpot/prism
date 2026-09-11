use std::borrow::Cow;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use tokio::net::TcpStream;

/// Disable Nagle on `sock`. Application-level batching already coalesces writes;
/// kernel coalescing would add extra delay on top.
pub fn set_nodelay(sock: &TcpStream) {
    if let Err(err) = sock.set_nodelay(true) {
        tracing::debug!(err = %err, "tcp nodelay failed");
    }
}

/// Normalize a bind/listen address.
///
/// Prism's config and docs commonly use the shorthand `":PORT"` to mean
/// "bind on all interfaces". Rust's `SocketAddr` parsing and Tokio bind APIs
/// do not accept `":PORT"`, so we normalize it to `"0.0.0.0:PORT"`.
pub fn normalize_bind_addr(addr: &str) -> Cow<'_, str> {
    let addr = addr.trim();
    if addr.starts_with(':') {
        Cow::Owned(format!("0.0.0.0{addr}"))
    } else {
        Cow::Borrowed(addr)
    }
}

/// Rewrite an unspecified bind address (`0.0.0.0` / `::`) to loopback for outbound connect.
///
/// `TcpListener::local_addr()` after binding `:8080` returns `0.0.0.0:8080`. Connecting to
/// that unspecified address fails on Windows (and is unreliable elsewhere). Same-process
/// in-band `$admin` proxying must dial loopback instead.
pub fn loopback_connect_addr(addr: SocketAddr) -> SocketAddr {
    if !addr.ip().is_unspecified() {
        return addr;
    }
    let ip = if addr.is_ipv6() {
        IpAddr::V6(Ipv6Addr::LOCALHOST)
    } else {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    };
    SocketAddr::new(ip, addr.port())
}

#[cfg(test)]
mod tests {
    use super::{loopback_connect_addr, normalize_bind_addr};
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

    #[test]
    fn normalize_bind_addr_port_only() {
        assert_eq!(normalize_bind_addr(":8080").as_ref(), "0.0.0.0:8080");
        assert_eq!(normalize_bind_addr(" :7000 ").as_ref(), "0.0.0.0:7000");
    }

    #[test]
    fn normalize_bind_addr_passthrough() {
        assert_eq!(
            normalize_bind_addr("127.0.0.1:8080").as_ref(),
            "127.0.0.1:8080"
        );
        assert_eq!(normalize_bind_addr("[::]:8080").as_ref(), "[::]:8080");
    }

    #[test]
    fn loopback_connect_addr_rewrites_unspecified() {
        let v4: SocketAddr = "0.0.0.0:8080".parse().unwrap();
        assert_eq!(
            loopback_connect_addr(v4),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 8080))
        );

        let v6: SocketAddr = "[::]:8080".parse().unwrap();
        assert_eq!(
            loopback_connect_addr(v6),
            SocketAddr::from((Ipv6Addr::LOCALHOST, 8080))
        );
    }

    #[test]
    fn loopback_connect_addr_preserves_specified() {
        let v4: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        assert_eq!(loopback_connect_addr(v4), v4);

        let lan: SocketAddr = "192.168.1.5:8080".parse().unwrap();
        assert_eq!(loopback_connect_addr(lan), lan);
    }
}
