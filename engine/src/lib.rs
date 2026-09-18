//! DiskSage engine — cross-platform disk stats and scan patterns.
//!
//! Exposed as a library so both the CLI binary (`main.rs`) and, in-process, the
//! Tauri desktop app can use the same logic (no bash, no subprocess). This is
//! the foundation for dropping the macOS-only bash `serve` dependency and making
//! the app work on Windows/Linux.

pub mod df;
pub mod reports;
pub mod scan;
pub mod serve;
pub mod settings;
pub mod util;
pub mod walk;
