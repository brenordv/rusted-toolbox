//! Shared machinery behind the `head` and `tail` tool crates: the GNU count
//! grammar, the byte-oriented input drivers and backward scan, and the
//! run-outcome plumbing both engines map to the same exit contract.

pub mod count_parser;
pub mod io_shared;
pub mod models;
