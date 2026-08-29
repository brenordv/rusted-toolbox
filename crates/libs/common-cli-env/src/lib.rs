//! Loads a tool's `.env` file into the process environment.
//!
//! [` load_env_variables `](load_env_variables::load_env_variables) searches the
//! executable's directory then the current working directory, loading the first
//! `.env` it finds. See its docs for the search order, the error contract, and
//! the trust caveats around the working-directory fallback.

pub mod load_env_variables;
