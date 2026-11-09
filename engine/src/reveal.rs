//! Reveal a path in the OS file manager (read-only: it just opens a window).
//!
//! The served UI runs on 127.0.0.1 — a different origin from the desktop
//! webview — so it can't call a Tauri API to do this. Instead the engine, which
//! already runs locally as the user, shells out to the platform's "reveal"
//! command. The serve layer only ever passes a path it is *currently* offering
//! as a finding (validated there), so this is never an arbitrary-path opener.

use std::path::Path;

/// Reveal `path` in the file manager, selected where the platform supports it:
/// macOS Finder (`open -R`), Windows Explorer (`/select`), Linux via `xdg-open`
/// (opens the folder — no portable "select").
pub fn reveal(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        run("open", &["-R".as_ref(), path.as_os_str()])
    }
    #[cfg(target_os = "windows")]
    {
        // `explorer /select,<path>` reveals and selects the item. explorer
        // returns a non-zero exit code even on success, so don't gate on status.
        let arg = format!("/select,{}", path.display());
        let _ = std::process::Command::new("explorer").arg(arg).spawn();
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        // No portable "reveal & select"; open the folder itself (or its parent
        // for a file) in the default file manager.
        let target = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        run("xdg-open", &[target.as_os_str()])
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = path;
        Err("reveal is not supported on this platform yet".into())
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn run(bin: &str, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let out = std::process::Command::new(bin)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}
