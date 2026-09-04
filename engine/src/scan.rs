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

use std::path::Path;

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

fn collect() -> Vec<Finding> {
    let mut findings = Vec::new();
    let home = match home_dir() {
        Some(h) => h,
        None => return findings,
    };

    // ollama_models — cross-platform (~/.ollama/models)
    let ollama = home.join(".ollama").join("models");
    if ollama.is_dir() {
        let size = dir_size(&ollama);
        if size > 10 * GIB {
            findings.push(Finding {
                id: "ollama_models",
                path: tildify(&ollama),
                size,
                severity: "medium",
                description: format!("Ollama models: {}", human(size)),
                action: "Remove unused models with 'ollama rm <model>' (re-pullable anytime).".into(),
            });
        }
    }

    // node_modules_aggregate — cross-platform (~/Development)
    let dev = home.join("Development");
    if dev.is_dir() {
        let size = node_modules_total(&dev);
        if size > 10 * GIB {
            findings.push(Finding {
                id: "node_modules_aggregate",
                path: tildify(&dev),
                size,
                severity: "low",
                description: format!("node_modules under ~/Development total {}", human(size)),
                action: "For projects you're done with: 'rm -rf node_modules' (reinstall anytime with your package manager).".into(),
            });
        }
    }

    findings
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
