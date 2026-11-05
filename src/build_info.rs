//! Authoritative application and build identity compiled into every binary.

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const BUILD_ID: &str = env!("RUSTNOTE_BUILD_ID");
pub const FULL_BUILD_IDENTIFIER: &str =
    concat!(env!("CARGO_PKG_VERSION"), "+", env!("RUSTNOTE_BUILD_ID"));
