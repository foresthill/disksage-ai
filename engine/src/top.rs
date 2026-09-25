//! Biggest folders under $HOME — the "where did it all go" view (DaisyDisk-like),
//! ported from the bash `cmd_top`. Part of the Full ("じっくり") scan.
//!
//! Roots: each top-level $HOME entry (Library shown by its subdirs instead), each
//! ~/Library subdir, and common large hidden caches. Each root's size is measured
//! in parallel (disjoint subtrees → wall-time ≈ the slowest root, not the sum).

use std::path::PathBuf;
use std::thread;

use crate::util::home_dir;
use crate::walk::{dir_size, file_size, tildify};

/// Hidden caches worth surfacing individually (they live outside ~/Library).
const HIDDEN: &[&str] = &[
    ".cache", ".ollama", ".docker", ".npm", ".yarn", ".gradle", ".m2", ".cargo", ".rustup",
    ".pyenv", ".nvm", ".bun",
];

fn roots() -> Vec<PathBuf> {
    let mut v = Vec::new();
    let home = match home_dir() {
        Some(h) => h,
        None => return v,
    };
    let library = home.join("Library");
    if let Ok(rd) = std::fs::read_dir(&home) {
        for e in rd.flatten() {
            let p = e.path();
            if p != library {
                v.push(p); // Library itself is represented by its subdirs below
            }
        }
    }
    if let Ok(rd) = std::fs::read_dir(&library) {
        for e in rd.flatten() {
            v.push(e.path());
        }
    }
    for d in HIDDEN {
        let p = home.join(d);
        if p.is_dir() {
            v.push(p);
        }
    }
    v
}

/// The `n` biggest roots, as (tildified path, bytes), largest first.
pub fn biggest_folders(n: usize) -> Vec<(String, u64)> {
    let handles: Vec<_> = roots()
        .into_iter()
        .map(|p| {
            thread::spawn(move || {
                let size = if p.is_dir() { dir_size(&p) } else { file_size(&p) };
                (tildify(&p), size)
            })
        })
        .collect();
    let mut out: Vec<(String, u64)> = handles.into_iter().filter_map(|h| h.join().ok()).collect();
    out.sort_by_key(|r| std::cmp::Reverse(r.1));
    out.truncate(n);
    out
}
