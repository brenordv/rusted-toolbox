//! dep-check-core: asks vulnerability and package-metadata services about one ecosystem's
//! packages and merges their answers into one [`report::Report`].
//!
//! [`engine::Checker`] runs a check; [`package::parse_specs`] turns typed lines into the
//! packages it takes. Every failure of a service ends inside the report, never as an error.

pub mod advisory;
pub mod ecosystem;
pub mod engine;
pub mod http;
pub mod merge;
pub mod package;
pub mod rate_limits;
pub mod render;
pub mod report;
pub mod sources;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
