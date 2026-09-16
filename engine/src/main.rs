// DiskSage engine — Rust port, PoC (Phase B).
//
// Reproduces bash commands in Rust one at a time, so each can be diffed against
// the bash implementation and the port grows while staying verifiable:
//   - `df`   — instant, scan-free disk view (this file)
//   - `scan` — directory-size patterns (see scan.rs)
//
// Cross-platform by design:
//   - disk enumeration uses `sysinfo` (works on macOS, Windows, Linux)
//   - APFS snapshot counting is macOS-only and lives behind a cfg gate; other
//     platforms simply report "not available" rather than shelling out.
//
// Usage:
//   disksage-engine df   [--json]
//   disksage-engine scan [--json]

mod scan;
mod util;
mod walk;

use std::collections::BTreeMap;
use sysinfo::Disks;
use util::human;

/// One mounted volume.
struct Volume {
    mount: String,
    used: u64,
    avail: u64,
}

/// Volumes grouped by total size — a proxy for "same APFS container", matching
/// the bash `render_disk_usage`, so one honest "X% full" is shown per disk.
fn containers() -> BTreeMap<u64, Vec<Volume>> {
    let disks = Disks::new_with_refreshed_list();
    let mut groups: BTreeMap<u64, Vec<Volume>> = BTreeMap::new();
    for disk in &disks {
        let total = disk.total_space();
        if total == 0 {
            continue;
        }
        let avail = disk.available_space();
        groups.entry(total).or_default().push(Volume {
            mount: disk.mount_point().to_string_lossy().into_owned(),
            used: total.saturating_sub(avail),
            avail,
        });
    }
    groups
}

/// Number of local APFS snapshots (the usual reason free space "shrinks by
/// itself"). macOS only; `None` elsewhere until those platforms are supported.
fn snapshot_count() -> Option<usize> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("tmutil")
            .args(["listlocalsnapshots", "/"])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        Some(text.lines().filter(|l| l.contains("com.apple")).count())
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// A container's label: the startup disk if it holds "/", else its biggest volume.
fn label_for(members: &[Volume]) -> &str {
    if members.iter().any(|m| m.mount == "/") {
        return "Startup disk";
    }
    members
        .iter()
        .max_by_key(|m| m.used)
        .map(|m| m.mount.as_str())
        .unwrap_or("disk")
}

fn cmd_df_human() {
    println!("## Current Disk Usage\n");
    for (total, members) in containers().iter().rev() {
        let free = members.iter().map(|m| m.avail).max().unwrap_or(0);
        let used = total.saturating_sub(free);
        let pct = if *total > 0 { used * 100 / total } else { 0 };
        let bar_len = 24u64;
        let fill = ((pct as f64) / 100.0 * bar_len as f64).round() as u64;
        let bar: String = "█".repeat(fill as usize) + &"░".repeat((bar_len - fill) as usize);
        println!("### {}", label_for(members));
        println!(
            "`{}`  used {} / {} ({}%) · free {}",
            bar,
            human(used),
            human(*total),
            pct,
            human(free)
        );
        if members.len() > 1 {
            println!("\n_Volumes below share this container's free space:_");
            let mut sorted: Vec<&Volume> = members.iter().collect();
            sorted.sort_by(|a, b| b.used.cmp(&a.used));
            for m in sorted {
                println!("- {} — {}", m.mount, human(m.used));
            }
        }
        println!();
    }
    match snapshot_count() {
        Some(0) => println!("Local snapshots: none"),
        Some(n) => println!(
            "Local snapshots: {n} — each one pins recently-deleted blocks, so free space can shrink on its own."
        ),
        None => println!("Local snapshots: n/a on this platform"),
    }
}

fn cmd_df_json() {
    // Deliberately dependency-free JSON so the PoC stays tiny; enough to diff.
    let mut items = String::new();
    for (total, members) in containers().iter().rev() {
        let free = members.iter().map(|m| m.avail).max().unwrap_or(0);
        let used = total.saturating_sub(free);
        let pct = if *total > 0 { used * 100 / total } else { 0 };
        if !items.is_empty() {
            items.push(',');
        }
        items.push_str(&format!(
            "{{\"label\":\"{}\",\"total\":{},\"used\":{},\"free\":{},\"pct\":{}}}",
            label_for(members).replace('"', "\\\""),
            total,
            used,
            free,
            pct
        ));
    }
    let snaps = match snapshot_count() {
        Some(n) => n.to_string(),
        None => "null".to_string(),
    };
    println!("{{\"containers\":[{items}],\"snapshots\":{snaps}}}");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    match args.first().map(String::as_str) {
        Some("df") => {
            if json {
                cmd_df_json();
            } else {
                cmd_df_human();
            }
        }
        Some("scan") => scan::run(json),
        Some("--version") | Some("-v") => {
            println!("disksage-engine {}", env!("CARGO_PKG_VERSION"));
        }
        _ => {
            eprintln!(
                "disksage-engine (PoC)\n\nUsage:\n  disksage-engine df [--json]\n  disksage-engine scan [--json]\n  disksage-engine --version"
            );
            std::process::exit(2);
        }
    }
}
