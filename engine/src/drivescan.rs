//! Whole-drive "where did it all go" scan via the NTFS $MFT — Windows only.
//!
//! This is the mechanism the Everything search tool uses. Walking folders one by
//! one (what the rest of DiskSage does) is slow on a full drive AND blind to
//! everything outside the user profile — so on Windows, where most of a full C:
//! drive lives in C:\Windows, C:\Program Files, pagefile.sys etc., the pattern
//! and Top scans see almost nothing. Reading the volume's Master File Table
//! yields every file's size and path in a single pass; we fold that into the
//! biggest top-level items and biggest individual files on the drive.
//!
//! Reading the raw volume needs an elevated (administrator) process — the same
//! requirement Everything has. Without it the open is denied and we report
//! `NeedsAdmin`, so the caller can tell the user to run as administrator (and
//! fall back to the home-scoped walk). Read-only, local, metadata only: no file
//! contents are read and nothing leaves the machine.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::path::Path;

/// One aggregated top-level item on the drive — a folder directly under the drive
/// root, or a file sitting at the root such as `pagefile.sys` — with total bytes.
pub struct Entry {
    pub name: String,
    pub bytes: u64,
}

/// Result of a whole-drive MFT scan.
pub struct DriveScan {
    pub drive: String,             // e.g. "C:"
    pub folders: Vec<Entry>,       // biggest top-level items, largest first
    pub files: Vec<(String, u64)>, // biggest individual files (path, bytes), largest first
    pub total_bytes: u64,
    pub file_count: u64,
}

/// Why a drive scan could not run.
pub enum DriveScanError {
    /// Not available on this platform (the MFT fast path is Windows-only).
    Unsupported,
    /// The raw volume read was denied — DiskSage must be run as administrator.
    NeedsAdmin,
    /// Any other NTFS / IO error.
    Failed(String),
}

/// Streaming aggregator: fold each file's (path, size) into top-level buckets and
/// a bounded top-N of the biggest individual files. Kept separate from the NTFS
/// reader so it is cross-platform and unit-testable — the reader glue below is the
/// only Windows-only part.
pub struct Aggregator {
    top_n: usize,
    by_top: HashMap<String, u64>,
    big: BinaryHeap<Reverse<(u64, String)>>, // min-heap capped at top_n
    total_bytes: u64,
    file_count: u64,
}

impl Aggregator {
    pub fn new(top_n: usize) -> Self {
        Self {
            top_n,
            by_top: HashMap::new(),
            big: BinaryHeap::new(),
            total_bytes: 0,
            file_count: 0,
        }
    }

    /// Record one file by its on-volume path and size.
    pub fn add(&mut self, path: &Path, size: u64) {
        self.file_count += 1;
        self.total_bytes += size;
        if let Some(top) = first_component(path) {
            *self.by_top.entry(top).or_insert(0) += size;
        }
        // Keep only the top_n biggest files. Build the owned path string only when
        // the file is actually a candidate (most files fail the cheap size test).
        if self.top_n == 0 {
            return;
        }
        if self.big.len() < self.top_n {
            self.big.push(Reverse((size, path.display().to_string())));
        } else if let Some(Reverse((min, _))) = self.big.peek() {
            if size > *min {
                self.big.pop();
                self.big.push(Reverse((size, path.display().to_string())));
            }
        }
    }

    pub fn finish(self, drive: String) -> DriveScan {
        let mut folders: Vec<Entry> = self
            .by_top
            .into_iter()
            .map(|(name, bytes)| Entry { name, bytes })
            .collect();
        folders.sort_by_key(|e| Reverse(e.bytes));
        folders.truncate(self.top_n);

        let mut files: Vec<(String, u64)> =
            self.big.into_iter().map(|Reverse((b, p))| (p, b)).collect();
        files.sort_by_key(|f| Reverse(f.1));

        DriveScan {
            drive,
            folders,
            files,
            total_bytes: self.total_bytes,
            file_count: self.file_count,
        }
    }
}

/// The top-level item name under the drive root. Works on the path strings that
/// ntfs-reader yields — `C:\Windows\x`, `\Windows\x`, or `Windows\x` — on its own
/// string handling (not the host platform's path parser), so it is deterministic
/// and unit-testable off Windows. `C:\pagefile.sys` → `pagefile.sys`.
fn first_component(path: &Path) -> Option<String> {
    let lossy = path.to_string_lossy();
    let mut s: &str = lossy.as_ref();
    // Strip an optional drive-letter prefix like "C:".
    if let [c, b':', ..] = s.as_bytes() {
        if c.is_ascii_alphabetic() {
            s = &s[2..];
        }
    }
    // Strip leading separators, then take up to the next separator.
    let s = s.trim_start_matches(['\\', '/']);
    let end = s.find(['\\', '/']).unwrap_or(s.len());
    let top = &s[..end];
    (!top.is_empty()).then(|| top.to_string())
}

/// Scan the system drive's MFT and return the biggest folders/files. Windows only.
#[cfg(windows)]
pub fn scan_system_drive(top_n: usize) -> Result<DriveScan, DriveScanError> {
    use ntfs_reader::{DefaultPathCache, FileInfo, Mft, Volume};

    // System drive, e.g. "C:" — the raw volume path must have no trailing "\".
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    let drive = drive.trim_end_matches('\\').to_string();
    let volume_path = format!(r"\\.\{drive}");

    // `Mft::new` holds the whole MFT in memory (~1 KiB/file); fine for an explicit
    // full-drive scan. Switch to `Mft::new_compact` / `MftScan` if memory bites.
    let volume = Volume::new(&volume_path).map_err(map_err)?;
    let mft = Mft::new(volume).map_err(map_err)?;

    let mut cache = DefaultPathCache::new();
    let mut agg = Aggregator::new(top_n);
    for file in mft.files() {
        let info = FileInfo::with_cache(&file, &mut cache);
        if info.is_directory {
            continue; // directories hold no data; the bytes live in their files
        }
        if let Some(path) = info.path.as_deref() {
            agg.add(path, info.size);
        }
    }
    Ok(agg.finish(drive))
}

/// Map an ntfs-reader error to NeedsAdmin (access denied) or a generic failure.
#[cfg(windows)]
fn map_err(e: ntfs_reader::NtfsReaderError) -> DriveScanError {
    let msg = e.to_string();
    if msg.to_lowercase().contains("denied") {
        DriveScanError::NeedsAdmin
    } else {
        DriveScanError::Failed(msg)
    }
}

#[cfg(not(windows))]
pub fn scan_system_drive(_top_n: usize) -> Result<DriveScan, DriveScanError> {
    Err(DriveScanError::Unsupported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn first_component_handles_prefixed_rooted_and_relative_paths() {
        assert_eq!(
            first_component(&PathBuf::from(r"C:\Windows\System32\x.dll")).as_deref(),
            Some("Windows")
        );
        assert_eq!(
            first_component(&PathBuf::from(r"C:\pagefile.sys")).as_deref(),
            Some("pagefile.sys")
        );
        assert_eq!(
            first_component(&PathBuf::from(r"\Users\me\x")).as_deref(),
            Some("Users")
        );
        assert_eq!(
            first_component(&PathBuf::from("Program Files/app/x")).as_deref(),
            Some("Program Files")
        );
    }

    #[test]
    fn aggregator_buckets_by_top_level_and_keeps_biggest_files() {
        let mut agg = Aggregator::new(2);
        agg.add(&PathBuf::from(r"C:\Windows\a"), 100);
        agg.add(&PathBuf::from(r"C:\Windows\b"), 50);
        agg.add(&PathBuf::from(r"C:\Users\c"), 200);
        agg.add(&PathBuf::from(r"C:\pagefile.sys"), 500);
        let scan = agg.finish("C:".to_string());

        assert_eq!(scan.file_count, 4);
        assert_eq!(scan.total_bytes, 850);
        // Top-level buckets, largest first, truncated to top_n = 2.
        assert_eq!(scan.folders.len(), 2);
        assert_eq!(scan.folders[0].name, "pagefile.sys");
        assert_eq!(scan.folders[0].bytes, 500);
        assert_eq!(scan.folders[1].name, "Users");
        assert_eq!(scan.folders[1].bytes, 200);
        // Biggest individual files (top_n = 2), largest first.
        assert_eq!(scan.files.len(), 2);
        assert_eq!(scan.files[0].1, 500);
        assert_eq!(scan.files[1].1, 200);
    }

    #[test]
    fn unsupported_off_windows() {
        // On non-Windows this returns Unsupported; on Windows it tries the volume.
        if cfg!(not(windows)) {
            assert!(matches!(
                scan_system_drive(10),
                Err(DriveScanError::Unsupported)
            ));
        }
    }
}
