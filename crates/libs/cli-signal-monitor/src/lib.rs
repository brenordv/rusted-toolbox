//! Installs a Ctrl-C (SIGINT) handler for graceful shutdown.
//!
//! [`setup_graceful_shutdown`](setup_graceful_shutdown::setup_graceful_shutdown)
//! either exits the process immediately on interrupt, or flips a shared
//! `AtomicBool` that the application polls to wind down its own work:
//!
//! ```no_run
//! use std::sync::atomic::Ordering;
//! use cli_signal_monitor::setup_graceful_shutdown::setup_graceful_shutdown;
//!
//! let shutdown = setup_graceful_shutdown(false);
//! while !shutdown.load(Ordering::Relaxed) {
//!     // do a unit of work, then re-check the flag
//! }
//! ```

pub mod setup_graceful_shutdown;
