//! The process-wide TLS crypto provider.

use rustls::crypto::aws_lc_rs::default_provider;

/// Sets rustls's aws-lc-rs provider as the process default. A build that enables more than one
/// rustls provider has no default otherwise, and `rustls::ClientConfig::builder()` panics
/// without one. Returns whether this call set it; a default already set is kept.
pub fn install_crypto_provider() -> bool {
    match default_provider().install_default() {
        Ok(()) => true,
        // The only error: a default is already set (the rejected provider comes back), and the
        // process keeps that one.
        Err(_rejected) => false,
    }
}
