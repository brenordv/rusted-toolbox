//! eh-gui: a desktop window over the eh capture engine and archive, built on Rune.
//!
//! `main.rs` parses the command line and boots logging; everything else lives here so the
//! binaries and the integration tests share one implementation.

pub mod app;
pub mod appearance;
pub mod bridge;
#[cfg(feature = "demo")]
pub mod demo;
pub mod design;
pub mod form;
pub mod read;
pub mod services;
pub mod state;
pub mod tasks;
pub mod views;
