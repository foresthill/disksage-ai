// Filesystem walking + sizing helpers shared by the scan patterns.

use std::path::Path;

use crate::util::home_dir;

/// On-disk size of a single entry. On Unix we count allocated 512-byte blocks
/// (`st_blocks`), which matches what `du` reports; elsewhere we fall back to the
/// logical length. Keeps the Rust numbers aligned with the bash engine on
/// macOS/Linux while staying portable to Windows.
#[cfg(unix)]
pub fn entry_size(meta: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.blocks() * 512
}
#[cfg(not(unix))]
pub fn entry_size(meta: &std::fs::Metadata) -> u64 {
    meta.len()
}

/// Recursive on-disk size of a directory (sum of every entry's allocated size).
/// Symlinks are not followed and unreadable entries are skipped, mirroring the
/// bash engine's tolerance of per-file permission errors.
pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return 0,
    };
    for entry in entries.flatten() {
        let ft = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if ft.is_symlink() {
            continue;
        }
        if let Ok(meta) = std::fs::symlink_metadata(entry.path()) {
            total += entry_size(&meta);
        }
        if ft.is_dir() {
            total += dir_size(&entry.path());
        }
    }
    total
}

/// Total size of every `node_modules` directory under `root` (not descending
/// into one once found — nested node_modules are already counted by their parent).
pub fn node_modules_total(root: &Path) -> u64 {
    let mut total = 0u64;
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return 0,
    };
    for entry in entries.flatten() {
        let ft = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if ft.is_symlink() || !ft.is_dir() {
            continue;
        }
        if entry.file_name() == "node_modules" {
            total += dir_size(&entry.path());
        } else {
            total += node_modules_total(&entry.path());
        }
    }
    total
}

/// On-disk size of a single file (0 if unreadable).
pub fn file_size(path: &Path) -> u64 {
    std::fs::symlink_metadata(path)
        .map(|m| entry_size(&m))
        .unwrap_or(0)
}

/// Recursively test whether a file named `name` exists anywhere under `root`
/// (like the bash `find <dir> -name Manifest.db`). Symlinks are not followed.
pub fn has_file_named(root: &Path, name: &str) -> bool {
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return false,
    };
    for e in entries.flatten() {
        let ft = match e.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            if has_file_named(&e.path(), name) {
                return true;
            }
        } else if e.file_name() == name {
            return true;
        }
    }
    false
}

/// Replace the home prefix with `~` for display, like the bash `tildify`.
pub fn tildify(path: &Path) -> String {
    if let Some(home) = home_dir() {
        if let Ok(rest) = path.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}
