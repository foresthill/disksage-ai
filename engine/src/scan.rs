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

use crate::lang::{is_ja, t};
use crate::util::{home_dir, human, json_escape};
use crate::walk::{dir_size, file_size, has_file_named, node_modules_total, tildify};

const GIB: u64 = 1024 * 1024 * 1024;

#[derive(Clone)]
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
            let desc = if is_ja() {
                format!("{vendor} iPhoneバックアップ: {}", human(size))
            } else {
                format!("{vendor} iPhone backup: {}", human(size))
            };
            f.push(Finding {
                id: "iphone_backup",
                path: tildify(&dir),
                size,
                severity: "medium",
                description: desc,
                action: t(
                    "If you no longer need this device backup, delete it (archive to an external drive first if unsure).",
                    "このデバイスのバックアップが不要なら削除（不安なら先に外付けへ退避）。",
                ).into(),
            });
        } else {
            let desc = if is_ja() {
                format!(
                    "{vendor} iPhoneバックアップ: {} — Manifest.db 無し・復元できない可能性大",
                    human(size)
                )
            } else {
                format!(
                    "{vendor} iPhone backup: {} — no Manifest.db, likely NOT restorable",
                    human(size)
                )
            };
            f.push(Finding {
                id: "iphone_backup",
                path: tildify(&dir),
                size,
                severity: "high",
                description: desc,
                action: t(
                    "A backup without Manifest.db can't be restored. Verify in the app; if orphaned, delete it to reclaim space.",
                    "Manifest.db が無いバックアップは復元不可。アプリで確認し、孤児なら削除して容量を回収。",
                ).into(),
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
        let desc = if is_ja() {
            format!("APFS ローカルスナップショット {n} 個（削除済みファイルを生かし続けます）")
        } else {
            format!("APFS local snapshots: {n} present (each keeps recently-deleted files alive)")
        };
        f.push(Finding {
            id: "apfs_snapshots",
            path: "/".into(),
            size: 0,
            severity: "high",
            description: desc,
            action: t(
                "List with 'tmutil listlocalsnapshots /'; delete an old one with 'sudo tmutil deletelocalsnapshots <date>'.",
                "'tmutil listlocalsnapshots /' で一覧、'sudo tmutil deletelocalsnapshots <日付>' で古いものを削除。",
            ).into(),
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
        let desc = if is_ja() {
            format!("macOS スワップ + sleepimage: {}", human(total))
        } else {
            format!("macOS swap + sleepimage: {}", human(total))
        };
        f.push(Finding {
            id: "vm_swap",
            path: "/private/var/vm".into(),
            size: total,
            severity: "medium",
            description: desc,
            action: t(
                "A reboot resets swap (sleepimage returns). If chronic, more RAM is the real fix.",
                "再起動でスワップは解消（sleepimage は戻ります）。慢性的ならメモリ増設が本筋。",
            )
            .into(),
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

    // --- Data-driven patterns (patterns/builtin.json + user extras) -------
    // The simple "a directory over a size threshold" rules. Bespoke patterns
    // (node_modules aggregate, Docker.raw, iPhone backup, snapshots, swap,
    // Electron caches) stay as code below.
    for pat in crate::patterns::all() {
        if let Some(dir) = pat.first_existing_dir(&home) {
            let size = dir_size(&dir);
            if size > pat.threshold {
                f.push(Finding {
                    id: pat.id,
                    path: tildify(&dir),
                    size,
                    severity: pat.severity,
                    description: format!("{}: {}", pat.label(), human(size)),
                    action: pat.action().to_string(),
                });
            }
        }
    }

    // node_modules is an aggregate walk rather than one directory.
    let dev = home.join("Development");
    if dev.is_dir() {
        let size = node_modules_total(&dev);
        finding_if_over(
            &mut f, dev, size, 10 * GIB, "node_modules_aggregate", "low",
            t("node_modules under ~/Development total", "~/Development 配下の node_modules 合計"),
            t(
                "For finished projects: 'rm -rf node_modules' (reinstall anytime with your package manager).",
                "終わったプロジェクトは 'rm -rf node_modules'（パッケージマネージャでいつでも再インストール可）。",
            ),
        );
    }

    // Docker.raw — a single VM disk file that never auto-shrinks.
    let docker = home.join("Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw");
    if docker.exists() {
        let size = file_size(&docker);
        finding_if_over(
            &mut f, docker, size, 10 * GIB, "docker_raw", "medium",
            t("Docker.raw VM disk", "Docker.raw VMディスク"),
            t(
                "Docker Desktop → Troubleshoot → Clean/Purge, or 'docker system prune -a --volumes'.",
                "Docker Desktop → Troubleshoot → Clean/Purge、または 'docker system prune -a --volumes'。",
            ),
        );
    }

    // --- Patterns with bespoke logic --------------------------------------
    check_iphone_backup(&mut f, &home); // Manifest.db corruption check
    check_apfs_snapshots(&mut f); // tmutil (macOS)
    check_vm_swap(&mut f); // /private/var/vm (macOS)
    check_electron_cache(&mut f, &home); // per-app Cache/Code Cache/GPUCache

    f
}

/// Electron apps' Cache / Code Cache / GPUCache under ~/Library/Application
/// Support/<app>, summed per app (safe to clear; the app path itself is kept, so
/// this is report-only — not in the one-click delete whitelist).
fn check_electron_cache(f: &mut Vec<Finding>, home: &Path) {
    let appsup = home.join("Library/Application Support");
    let rd = match std::fs::read_dir(&appsup) {
        Ok(r) => r,
        Err(_) => return,
    };
    for e in rd.flatten() {
        let d = e.path();
        if !d.is_dir() {
            continue;
        }
        let mut total = 0u64;
        for cache in ["Cache", "Code Cache", "GPUCache"] {
            let cdir = d.join(cache);
            if cdir.is_dir() {
                total += dir_size(&cdir);
            }
        }
        if total > 500 * 1024 * 1024 {
            let app = d
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let desc = if is_ja() {
                format!("{app} の Electron キャッシュ: {}", human(total))
            } else {
                format!("{app} Electron caches: {}", human(total))
            };
            f.push(Finding {
                id: "electron_cache",
                path: tildify(&d),
                size: total,
                severity: "safe",
                description: desc,
                action: t(
                    "Quit the app, then delete its Cache / Code Cache / GPUCache folders.",
                    "アプリを終了し、その Cache / Code Cache / GPUCache フォルダを削除。",
                )
                .into(),
            });
        }
    }
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
