//! Shared Azure operations for the workspace tools: describing and building
//! Entra ID credentials (`auth`), fetching Key Vault secrets (`key_vault`), and
//! checking service hosts against Azure DNS suffixes (`host`).

pub mod auth;
pub mod host;
pub mod key_vault;
