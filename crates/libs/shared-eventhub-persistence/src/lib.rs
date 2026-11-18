//! Everything EventHub tools keep on disk: the message archive (a redb
//! database per data folder), search over it, the txt/csv/json exporters, the
//! disk-write modes, `config.json` with its local secrets store, and the
//! workspace of saved connection profiles.

pub mod config;
pub mod export;
pub mod paths;
pub mod search;
pub mod secrets;
pub mod store;
pub mod workspace;
