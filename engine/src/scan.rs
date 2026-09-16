// Scan patterns — Rust port, starting with the OS-agnostic ones.
//
// This is where the bash `scan` engine begins moving to Rust. Two patterns to
// start, both just "measure a directory, compare to a threshold, emit a
// finding" — the core primitive every pattern shares:
//   - ollama_models        ~/.ollama/models   (> 10 GB)   — cross-platform
//   - node_modules_aggregate  ~/Development    (> 10 GB)   — cross-platform
//
// macOS-specific patterns (tmutil snapshots, iOS backups, CoreSimulator, …)
// come later, behind cfg gates, once this primitive is proven against bash.

use std::path::{Path, PathBuf};

use crate::util::{home_dir, human, json_escape};

const GIB: u64 = 1024 * 1024 * 1024;

pub struct Finding {
    pub id: &'static str,
    pub path: String,
    pub size: u64,
    pub severity: &'static str,
    pub description: String,
    pub action: String,
}

/// On-disk size of a single entry. On Unix we count allocated 512-byte blocks
/// (`st_blocks`), which matches what `du` reports; elsewhere we fall back to the
/// logical length. This keeps the Rust numbers aligned with the bash engine's
/// `du`-based sizes on macOS/Linux while staying portable to Windows.
#[cfg(unix)]
fn entry_size(meta: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.blocks() * 512
}
#[cfg(not(unix))]
fn entry_size(meta: &std::fs::Metadata) -> u64 {
    meta.len()
}

/// Recursive on-disk size of a directory (sum of every entry's allocated size).
/// Symlinks are not followed and unreadable entries are skipped, mirroring the
/// bash engine's tolerance of per-file permission errors.
fn dir_size(path: &Path) -> u64 {
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
fn node_modules_total(root: &Path) -> u64 {
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

/// Replace the home prefix with `~` for display, like the bash `tildify`.
fn tildify(path: &Path) -> String {
    if let Some(home) = home_dir() {
        if let Ok(rest) = path.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}

/// On-disk size of a single file (0 if unreadable).
fn file_size(path: &Path) -> u64 {
    std::fs::symlink_metadata(path).map(|m| entry_size(&m)).unwrap_or(0)
}

/// Push a finding when `size` exceeds `threshold`. Keeps each pattern to one line.
#[allow(clippy::too_many_arguments)]
fn finding_if_over(
    f: &mut Vec<Finding>,
    path: PathBuf,
    size: u64,
    threshold: u64,
    id: &'static str,
    severity: &'static str,
    label: &str,
    action: &str,
) {
    if size > threshold {
        f.push(Finding {
            id,
            path: tildify(&path),
            size,
            severity,
            description: format!("{label}: {}", human(size)),
            action: action.into(),
        });
    }
}

/// Convenience for the common "a directory over a size threshold" pattern.
/// If the directory is absent (e.g. a macOS path on Linux), it's simply skipped.
#[allow(clippy::too_many_arguments)]
fn dir_pattern(
    f: &mut Vec<Finding>,
    path: PathBuf,
    threshold: u64,
    id: &'static str,
    severity: &'static str,
    label: &str,
    action: &str,
) {
    if path.is_dir() {
        let size = dir_size(&path);
        finding_if_over(f, path, size, threshold, id, severity, label, action);
    }
}

fn collect() -> Vec<Finding> {
    let mut f = Vec::new();
    let home = match home_dir() {
        Some(h) => h,
        None => return f,
    };

    // --- Cross-platform ---------------------------------------------------
    dir_pattern(
        &mut f, home.join(".ollama/models"), 10 * GIB, "ollama_models", "medium",
        "Ollama models", "Remove unused models with 'ollama rm <model>' (re-pullable anytime).",
    );
    dir_pattern(
        &mut f, home.join(".cache"), 5 * GIB, "user_cache", "safe",
        "~/.cache dev-tool caches",
        "Clear per tool (uv cache clean, etc.); ~/.cache is re-downloaded on demand.",
    );
    // node_modules is an aggregate walk rather than one directory.
    let dev = home.join("Development");
    if dev.is_dir() {
        let size = node_modules_total(&dev);
        finding_if_over(
            &mut f, dev, size, 10 * GIB, "node_modules_aggregate", "low",
            "node_modules under ~/Development total",
            "For finished projects: 'rm -rf node_modules' (reinstall anytime with your package manager).",
        );
    }

    // --- macOS paths (absent on other OSes → naturally skipped) -----------
    dir_pattern(
        &mut f, home.join("Library/Caches"), 5 * GIB, "library_caches", "safe",
        "~/Library/Caches app caches",
        "Quit the app, then clear its subfolder (or brew cleanup / yarn cache clean).",
    );
    dir_pattern(
        &mut f, home.join("Library/Developer/Xcode/DerivedData"), 5 * GIB, "xcode_derived_data",
        "safe", "Xcode DerivedData", "Delete it; Xcode rebuilds on the next build.",
    );
    dir_pattern(
        &mut f, home.join("Library/Developer/Xcode/iOS DeviceSupport"), 3 * GIB,
        "ios_devicesupport", "safe", "Xcode iOS DeviceSupport",
        "Delete old versions; re-downloaded when you next connect that device.",
    );
    dir_pattern(
        &mut f, home.join("Library/Developer/CoreSimulator/Caches"), GIB, "coresimulator_caches",
        "safe", "CoreSimulator caches", "Safe to clear; regenerated by the simulator.",
    );

    // Docker.raw — a single VM disk file that never auto-shrinks.
    let docker = home.join("Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw");
    if docker.exists() {
        let size = file_size(&docker);
        finding_if_over(
            &mut f, docker, size, 10 * GIB, "docker_raw", "medium", "Docker.raw VM disk",
            "Docker Desktop → Troubleshoot → Clean/Purge, or 'docker system prune -a --volumes'.",
        );
    }

    f
}

pub fn run(json: bool) {
    let findings = collect();
    if json {
        let mut items = String::new();
        for f in &findings {
            if !items.is_empty() {
                items.push(',');
            }
            items.push_str(&format!(
                "{{\"id\":\"{}\",\"path\":\"{}\",\"size\":{},\"severity\":\"{}\",\"description\":\"{}\",\"action\":\"{}\"}}",
                f.id,
                json_escape(&f.path),
                f.size,
                f.severity,
                json_escape(&f.description),
                json_escape(&f.action),
            ));
        }
        println!("{{\"findings\":[{items}]}}");
        return;
    }

    println!("## Findings\n");
    if findings.is_empty() {
        println!("No findings above threshold.");
        return;
    }
    for f in &findings {
        println!("- [{}] {}", f.severity, f.description);
        println!("  path: {}", f.path);
        println!("  size: {}", human(f.size));
        println!("  action: {}\n", f.action);
    }
}
