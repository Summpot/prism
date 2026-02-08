//! Tunnel mode (reverse connection), inspired by frp.
//!
//! This module is a Rust port of the existing Go implementation under `internal/tunnel/*`
//! and follows the wire format described in `DESIGN.md` (Tunnel wire protocol v1).

pub mod autolisten;
pub mod client;
pub mod datagram;
pub mod manager;
pub mod protocol;
pub mod server;
pub mod transport;
