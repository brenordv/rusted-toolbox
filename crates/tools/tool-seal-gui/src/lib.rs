//! seal-gui: a window to seal and unseal text and files with age keys, on Rune.

pub mod cli_utils;
pub mod config;
pub mod design;
pub mod seal_gui_app;
pub mod services;
pub mod state;
#[cfg(test)]
pub(crate) mod test_support;
pub mod views;
pub mod worker;
