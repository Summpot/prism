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
/// that unspecified address fails on Windows (and is unreliable elsewhere).
#[allow(dead_code)]
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

    #[test]
    fn test_is_forbidden_ssrf_ip() {
        use super::is_forbidden_ssrf_ip;
        use std::net::IpAddr;

        // Forbidden IPv4
        assert!(is_forbidden_ssrf_ip("169.254.169.254".parse::<IpAddr>().unwrap()));
        assert!(is_forbidden_ssrf_ip("0.0.0.0".parse::<IpAddr>().unwrap()));
        assert!(is_forbidden_ssrf_ip("0.1.2.3".parse::<IpAddr>().unwrap()));
        assert!(is_forbidden_ssrf_ip("255.255.255.255".parse::<IpAddr>().unwrap()));
        assert!(is_forbidden_ssrf_ip("224.0.0.1".parse::<IpAddr>().unwrap()));
        assert!(is_forbidden_ssrf_ip("100.64.0.1".parse::<IpAddr>().unwrap()));
        assert!(is_forbidden_ssrf_ip("198.18.0.1".parse::<IpAddr>().unwrap()));

        // Forbidden IPv4-mapped IPv6
        assert!(is_forbidden_ssrf_ip("::ffff:169.254.169.254".parse::<IpAddr>().unwrap()));
        assert!(is_forbidden_ssrf_ip("::ffff:0.0.0.0".parse::<IpAddr>().unwrap()));

        // Forbidden IPv6
        assert!(is_forbidden_ssrf_ip("::".parse::<IpAddr>().unwrap()));
        assert!(is_forbidden_ssrf_ip("fe80::1".parse::<IpAddr>().unwrap()));
        assert!(is_forbidden_ssrf_ip("fc00::1".parse::<IpAddr>().unwrap()));
        assert!(is_forbidden_ssrf_ip("fd12:3456:789a::1".parse::<IpAddr>().unwrap()));
        assert!(is_forbidden_ssrf_ip("ff02::1".parse::<IpAddr>().unwrap()));

        // Allowed public IPs
        assert!(!is_forbidden_ssrf_ip("1.1.1.1".parse::<IpAddr>().unwrap()));
        assert!(!is_forbidden_ssrf_ip("8.8.8.8".parse::<IpAddr>().unwrap()));
        assert!(!is_forbidden_ssrf_ip("2606:4700:4700::1111".parse::<IpAddr>().unwrap()));
    }
}

/// Check if an IP address belongs to link-local, cloud metadata, unspecified, broadcast,
/// multicast, carrier-grade NAT, or IPv6 ULA reserved ranges. Handles IPv4-mapped IPv6 canonically.
pub fn is_forbidden_ssrf_ip(ip: IpAddr) -> bool {
    let canonical = ip.to_canonical();
    match canonical {
        IpAddr::V4(v4) => {
            // Link-local / AWS IMDS / Cloud metadata (169.254.0.0/16)
            if v4.is_link_local() {
                return true;
            }
            // Current network / unspecified (0.0.0.0/8)
            if v4.is_unspecified() || v4.octets()[0] == 0 {
                return true;
            }
            // Broadcast (255.255.255.255)
            if v4.is_broadcast() {
                return true;
            }
            // Multicast (224.0.0.0/4)
            if v4.is_multicast() {
                return true;
            }
            // Carrier-grade NAT (100.64.0.0/10)
            if v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64 {
                return true;
            }
            // Benchmark / documentation (198.18.0.0/15, 192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24)
            if v4.octets()[0] == 198 && (v4.octets()[1] == 18 || v4.octets()[1] == 19) {
                return true;
            }
            false
        }
        IpAddr::V6(v6) => {
            // Unspecified (::)
            if v6.is_unspecified() {
                return true;
            }
            // IPv6 link-local (fe80::/10)
            let seg0 = v6.segments()[0];
            if (seg0 & 0xffc0) == 0xfe80 {
                return true;
            }
            // IPv6 Unique Local Address (ULA fc00::/7)
            if (seg0 & 0xfe00) == 0xfc00 {
                return true;
            }
            // IPv6 Multicast (ff00::/8)
            if v6.is_multicast() {
                return true;
            }
            false
        }
    }
}
