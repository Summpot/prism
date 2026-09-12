//! In-band `$admin` control protocol.
//!
//! Yamux stream (`$admin` + PRPX) → length-delimited frames → postcard envelopes.
//! No HTTP, no loopback dial.

mod channel;
mod codec;
mod types;

pub use channel::{
    AdminCallContext, AdminControl, AdminEventWatches, ControlChannel, client_features, connect,
    serve, serve_with_shared_identity,
};
pub use types::*;
