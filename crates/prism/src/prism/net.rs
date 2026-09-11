use std::borrow::Cow;

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

#[cfg(test)]
mod tests {
    use super::normalize_bind_addr;

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
}
