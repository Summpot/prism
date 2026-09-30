pub mod ffi;
pub mod prism;

pub use ffi::*;
pub use prism::run;

uniffi::setup_scaffolding!("prism_native");
