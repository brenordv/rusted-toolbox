//! File-system helpers for the toolbox: recursive listing, content-based
//! binary detection, owner-only directory/file creation, atomic
//! temp-file-then-rename replacement, and moving a tool's data from its old
//! place to its new one.

pub mod adoption;
pub mod atomic_write;
pub mod binary_sniff;
pub mod file_system;
pub mod permissions;
