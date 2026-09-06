//! Tunnel mode (reverse connection), inspired by frp.
//!
//! This module was originally ported from the legacy Go implementation (now removed)
//! and follows the tunnel wire protocol v1 format used by Prism.

pub mod autolisten;
pub mod client;
pub mod connector;
pub mod datagram;
pub mod fake_lan;
pub mod local_proxy;
pub mod manager;
pub mod mdns;
pub mod optimizer;
pub mod protocol;
pub mod server;
pub mod transport;
