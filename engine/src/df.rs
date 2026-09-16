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

/// Number of local APFS snapshots (the usual reason free space "shrinks by
/// itself"). macOS only; `None` elsewhere until those platforms are supported.
pub fn snapshot_count() -> Option<usize> {
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
