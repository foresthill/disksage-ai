//! Disk free-space data — used by the `df` command and, in-process, by the
//! desktop app's menu bar tray.

use std::collections::BTreeMap;
use std::path::Path;

use sysinfo::Disks;

/// One mounted volume.
pub struct Volume {
    pub mount: String,
    pub used: u64,
    pub avail: u64,
}

/// Volumes grouped by total size — a proxy for "same APFS container", matching
/// the bash `render_disk_usage`, so one honest "X% full" is shown per disk.
pub fn containers() -> BTreeMap<u64, Vec<Volume>> {
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

/// A breakdown of local APFS snapshots — the usual reason free space "shrinks by
/// itself", since each one pins recently-deleted blocks until it is released.
#[derive(Default)]
pub struct Snapshots {
    pub total: usize,
    /// `com.apple.os.update-*` — staged while a macOS update is pending; these
    /// clear automatically once the update finishes.
    pub os_update: usize,
}

/// Local APFS snapshots on "/". Uses `diskutil apfs listSnapshots`, which — unlike
/// `tmutil listlocalsnapshots` — also reports the `com.apple.os.update-*` snapshot
/// that OS-update staging leaves behind. That one is a common *hidden* space
/// holder: it can pin many GB that freeing files won't return until the update
/// completes. macOS only; `None` elsewhere or if the command isn't available.
pub fn snapshots() -> Option<Snapshots> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("diskutil")
            .args(["apfs", "listSnapshots", "/"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let mut s = Snapshots::default();
        for line in text.lines() {
            if let Some(name) = line.trim().strip_prefix("Name:") {
                s.total += 1;
                if name.contains("os.update") {
                    s.os_update += 1;
                }
            }
        }
        Some(s)
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Number of local APFS snapshots (used by the `df` CLI). Delegates to
/// `snapshots()` so it benefits from the diskutil-based detection.
pub fn snapshot_count() -> Option<usize> {
    snapshots().map(|s| s.total)
}

/// A container's label: the startup disk if it holds "/", else its biggest volume.
pub fn label_for(members: &[Volume]) -> &str {
    if members.iter().any(|m| m.mount == "/") {
        return "Startup disk";
    }
    members
        .iter()
        .max_by_key(|m| m.used)
        .map(|m| m.mount.as_str())
        .unwrap_or("disk")
}

/// Free bytes of the startup disk (mount "/"), else the largest disk. A single
/// number for the menu bar tray, computed in-process (no subprocess).
pub fn startup_free() -> u64 {
    let disks = Disks::new_with_refreshed_list();
    disks
        .iter()
        .find(|d| d.mount_point() == Path::new("/"))
        .or_else(|| disks.iter().max_by_key(|d| d.total_space()))
        .map(|d| d.available_space())
        .unwrap_or(0)
}
