//! Recently-grown large files — the "what just ate my disk this month" view,
//! ported from the bash `find_flow_type_files`. Part of the Full ("じっくり") scan.
//!
//! Walks $HOME for files > 500 MB modified in the last 30 days, pruning the big,
//! low-signal trees (node_modules, .git, Caches, .cache) that aggregate patterns
//! already cover — the same prune the bash version uses.

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::util::home_dir;
use crate::walk::tildify;

const MIN_SIZE: u64 = 500 * 1024 * 1024; // 500 MB
const WINDOW: Duration = Duration::from_secs(30 * 24 * 60 * 60); // 30 days

fn pruned(name: &str) -> bool {
    matches!(name, "node_modules" | ".git" | "Caches" | ".cache")
}

fn walk(dir: &Path, now: SystemTime, out: &mut Vec<(u64, String)>) {
    let rd = match fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return, // permission denied etc. — skip, like the bash `find`
    };
    for e in rd.flatten() {
        let path = e.path();
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let ft = meta.file_type();
        if ft.is_symlink() {
            continue; // never follow symlinks (avoids cycles / double counting)
        }
        if ft.is_dir() {
            if !pruned(&e.file_name().to_string_lossy()) {
                walk(&path, now, out);
            }
        } else if ft.is_file() && meta.len() > MIN_SIZE {
            if let Ok(modified) = meta.modified() {
                if now.duration_since(modified).map(|d| d < WINDOW).unwrap_or(false) {
                    out.push((meta.len(), tildify(&path)));
                }
            }
        }
    }
}

/// The `n` largest recently-grown files, as (bytes, tildified path), largest first.
pub fn recent_large(n: usize) -> Vec<(u64, String)> {
    let home = match home_dir() {
        Some(h) => h,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    walk(&home, SystemTime::now(), &mut out);
    out.sort_by_key(|r| std::cmp::Reverse(r.0));
    out.truncate(n);
    out
}

/// True for photo/video/audio/creative files — the ones you should archive to an
/// external drive rather than just delete (they can't be regenerated).
pub fn is_media(path: &str) -> bool {
    const EXTS: &[&str] = &[
        ".mov", ".mp4", ".m4v", ".avi", ".mkv", ".heic", ".jpg", ".jpeg", ".png", ".gif", ".tiff",
        ".raw", ".wav", ".aiff", ".flac", ".mp3", ".m4a", ".logicx", ".als", ".psd", ".ai", ".prproj",
    ];
    let lower = path.to_lowercase();
    EXTS.iter().any(|e| lower.ends_with(e))
}
