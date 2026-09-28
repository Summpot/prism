//! In-band `$control` protocol.
//!
//! Yamux stream (`$control` + PRPX) → length-delimited frames → postcard envelopes.
//! Direct binary RPC, no pseudo-REST translation, no loopback dial.

mod channel;
mod codec;
mod types;

#[allow(unused_imports)]
pub use channel::{
    ControlCallContext, ControlChannel, ControlEventWatches, ControlHandler, client_features,
    connect, serve, serve_with_shared_identity,
};
pub use types::*;
