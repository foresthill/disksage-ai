//! Deletion for the served UI — always to the Trash, never `rm`, and only for a
//! whitelist of patterns whose finding path IS the thing to delete. This upholds
//! DiskSage's rules: never auto-delete, always via the Trash (recoverable), and
//! never an arbitrary path.

use std::path::{Path, PathBuf};

use crate::util::home_dir;

/// Patterns whose finding path is exactly the thing to move to Trash. Excludes,
/// on purpose: electron_cache (path is the app bundle), docker_raw (risky while
/// Docker runs), node_modules_aggregate (path is ~/Development), and the caches /
/// snapshots / swap patterns (need a tool-native clean, tmutil or a reboot).
pub const DELETABLE: &[&str] = &[
    "iphone_backup",
    "ollama_models",
    "xcode_derived_data",
    "coresimulator_caches",
    "coresimulator_devices",
    "ios_devicesupport",
];

pub fn is_deletable(id: &str) -> bool {
    DELETABLE.contains(&id)
}

/// Expand a leading `~/` (finding paths are tildified for display) to a real path.
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

/// Move a path to the Trash (never delete outright): macOS via Finder, Linux via
/// `gio trash` or `trash` (trash-cli). Other platforms are not supported yet.
pub fn to_trash(path: &Path) -> Result<(), String> {
    let p = path.to_string_lossy().to_string();
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "tell application \"Finder\" to delete POSIX file \"{}\"",
            p.replace('\\', "\\\\").replace('"', "\\\"")
        );
        let out = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }
    #[cfg(target_os = "linux")]
    {
        let attempts: [(&str, &[&str]); 2] = [("gio", &["trash", &p]), ("trash", &[&p])];
        for (bin, args) in attempts {
            if let Ok(out) = std::process::Command::new(bin).args(args).output() {
                if out.status.success() {
                    return Ok(());
                }
            }
        }
        Err("no trash tool found (install trash-cli or gio)".into())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = p;
        Err("trash is not supported on this platform yet".into())
    }
}
