//! The process crypto provider. This is the only test in its binary: the provider is
//! process-global. Under `cargo test --workspace` rustls carries both the ring and the
//! aws-lc-rs provider, and `ClientConfig::builder()` panics unless a default is installed.

use eh_core::tls::install_crypto_provider;
use rustls::crypto::CryptoProvider;

#[test]
fn the_first_install_sets_the_default_a_second_keeps_it_and_tls_configs_build() {
    let first = install_crypto_provider();
    let installed = CryptoProvider::get_default().is_some();
    let second = install_crypto_provider();

    let _config = rustls::ClientConfig::builder();

    assert!(first);
    assert!(installed);
    assert!(!second);
}
