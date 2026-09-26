//! DiskSage engine — cross-platform disk stats and scan patterns.
//!
//! Exposed as a library so both the CLI binary (`main.rs`) and, in-process, the
//! Tauri desktop app can use the same logic (no bash, no subprocess). This is
//! the foundation for dropping the macOS-only bash `serve` dependency and making
//! the app work on Windows/Linux.

pub mod ai;
pub mod ai_ui;
pub mod audit;
pub mod breakdown;
pub mod claude_cli;
pub mod deep;
pub mod df;
pub mod findings;
pub mod flow;
pub mod lang;
pub mod mask;
pub mod page;
pub mod report;
pub mod reports;
pub mod scan;
pub mod secret;
pub mod serve;
pub mod settings;
pub mod top;
pub mod trash;
pub mod util;
pub mod walk;
