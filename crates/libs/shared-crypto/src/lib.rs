pub mod engine;
pub mod errors;
pub mod keys;

// Load-bearing re-exports: phases 2-5 keep their "zero `age::` imports" rule
// only through these names.
pub use age::secrecy::SecretString;
pub use age::x25519::{Identity, Recipient};
