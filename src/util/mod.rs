/// Payloads above this run CPU-bound work (compression, envelope JSON parse) via `block_in_place` instead of inline on the async worker.
pub const INLINE_CPU_MAX_BYTES: usize = 64 * 1024;

pub mod crypto;
pub mod encoding;
pub mod network;
pub mod sliding_window;
pub mod ssrf;
pub mod stats;
pub mod throttle;
