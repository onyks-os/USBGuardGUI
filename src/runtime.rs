//! The shared Tokio runtime (docs/architecture.md §5.2).
//!
//! GTK owns the main thread and its loop, so there is no Tokio runtime in that
//! thread's context and a bare `tokio::spawn` would panic. The runtime is
//! created explicitly, once, and reached through [`runtime`]. `clippy.toml`
//! bans `tokio::spawn` so the rule is enforced by the linter, not by memory.

use std::sync::OnceLock;

use tokio::runtime::{Builder, Runtime};

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// The shared I/O runtime. Two workers suffice: the entire workload is
/// I/O-bound on a single D-Bus connection.
///
/// # Panics
///
/// If the operating system refuses to create the worker threads. There is no
/// way to continue without them, and it happens before any window exists.
#[allow(clippy::expect_used)] // see "Panics": nothing can run without the runtime
pub fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all() // I/O reactor and timer are both required
            .thread_name("usbguard-io")
            .build()
            .expect("failed to build the Tokio runtime")
    })
}
