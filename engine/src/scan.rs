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
use crate::walk::{dir_size, file_size, has_file_named, node_modules_total, tildify};

const GIB: u64 = 1024 * 1024 * 1024;

pub struct Finding {
    pub id: &'static str,
    pub path: String,
    pub size: u64,
    pub severity: &'static str,
    pub description: String,
    pub action: String,
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

/// iPhone backups (Apple's location + common third-party apps). A backup without
/// Manifest.db cannot be restored — flagged high. macOS paths; skipped elsewhere.
fn check_iphone_backup(f: &mut Vec<Finding>, home: &Path) {
    let appsup = home.join("Library/Application Support");
    let vendors: [(&str, PathBuf); 6] = [
        ("Apple", appsup.join("MobileSync/Backup")),
        ("iMobie", appsup.join("iMobie")),
        ("AnyTrans", appsup.join("AnyTrans")),
        ("3uTools", appsup.join("3uTools")),
        ("DearMob", appsup.join("DearMob")),
        ("iMazing", appsup.join("iMazing")),
    ];
    for (vendor, dir) in vendors {
        if !dir.is_dir() {
            continue;
        }
        let size = dir_size(&dir);
        if size <= GIB {
            continue; // ignore < 1 GB (empty shells / cache-only installs)
        }
        if has_file_named(&dir, "Manifest.db") {
            f.push(Finding {
                id: "iphone_backup",
                path: tildify(&dir),
                size,
                severity: "medium",
                description: format!("{vendor} iPhone backup: {}", human(size)),
                action: "If you no longer need this device backup, delete it (archive to an external drive first if unsure).".into(),
            });
        } else {
            f.push(Finding {
                id: "iphone_backup",
                path: tildify(&dir),
                size,
                severity: "high",
                description: format!("{vendor} iPhone backup: {} — no Manifest.db, likely NOT restorable", human(size)),
                action: "A backup without Manifest.db can't be restored. Verify in the app; if orphaned, delete it to reclaim space.".into(),
            });
        }
    }
}

/// APFS local snapshots (Time Machine keeps deleted files alive). macOS only.
#[cfg(target_os = "macos")]
fn check_apfs_snapshots(f: &mut Vec<Finding>) {
    let out = match std::process::Command::new("tmutil")
        .args(["listlocalsnapshots", "/"])
        .output()
    {
        Ok(o) => o,
        Err(_) => return,
    };
    let n = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.contains("com.apple.TimeMachine"))
        .count();
    if n > 3 {
        f.push(Finding {
            id: "apfs_snapshots",
            path: "/".into(),
            size: 0,
            severity: "high",
            description: format!("APFS local snapshots: {n} present (each keeps recently-deleted files alive)"),
            action: "List with 'tmutil listlocalsnapshots /'; delete an old one with 'sudo tmutil deletelocalsnapshots <date>'.".into(),
        });
    }
}
#[cfg(not(target_os = "macos"))]
fn check_apfs_snapshots(_f: &mut Vec<Finding>) {}

/// macOS swap + sleepimage under /private/var/vm. macOS only.
#[cfg(target_os = "macos")]
fn check_vm_swap(f: &mut Vec<Finding>) {
    let vmdir = Path::new("/private/var/vm");
    if !vmdir.is_dir() {
        return;
    }
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(vmdir) {
        for e in entries.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("swapfile") || name == "sleepimage" {
                total += file_size(&e.path());
            }
        }
    }
    if total > 5 * GIB {
        f.push(Finding {
            id: "vm_swap",
            path: "/private/var/vm".into(),
            size: total,
            severity: "medium",
            description: format!("macOS swap + sleepimage: {}", human(total)),
            action: "A reboot resets swap (sleepimage returns). If chronic, more RAM is the real fix.".into(),
        });
    }
}
#[cfg(not(target_os = "macos"))]
fn check_vm_swap(_f: &mut Vec<Finding>) {}

pub fn collect() -> Vec<Finding> {
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

    // --- Patterns with bespoke logic --------------------------------------
    check_iphone_backup(&mut f, &home); // Manifest.db corruption check
    check_apfs_snapshots(&mut f); // tmutil (macOS)
    check_vm_swap(&mut f); // /private/var/vm (macOS)

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
