//! Shared helpers that pull in heavier dependencies than `common-utils` allows.
//!
//! Kept separate from `common-utils` (which stays dependency-light) because each
//! helper needs an external crate: clipboard access ([`copy_to_clipboard`]) via
//! `arboard`, GUID generation ([`new_guid`]) via `uuid`, and non-printable
//! stripping ([`clean_str_regex`]) via `regex`.

pub mod copy_string_to_clipboard;
pub mod new_guid;
pub mod sanitize_str_regex;

pub use copy_string_to_clipboard::copy_to_clipboard;
pub use new_guid::new_guid;
pub use sanitize_str_regex::clean_str_regex;
