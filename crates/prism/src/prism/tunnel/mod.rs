//! Tunnel mode (reverse connection), inspired by frp.
//!
//! This module was originally ported from the legacy Go implementation (now removed)
//! and follows the wire format described in `DESIGN.md` (Tunnel wire protocol v1).

pub mod autolisten;
pub mod client;
pub mod datagram;
pub mod manager;
pub mod protocol;
pub mod server;
pub mod transport;
