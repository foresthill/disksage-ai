//! Drill-down for aggregate findings: instead of one un-deletable
//! "node_modules total / Caches total" line, show the actual sub-items with
//! sizes and per-item checkboxes, so you can pick exactly what to move to Trash.
//!
//! Two kinds:
//!   - NodeModules — every `node_modules` dir under ~/Development (not the copies
//!     nested inside another node_modules, to avoid double counting)
//!   - LibraryCaches — each app's subfolder directly under ~/Library/Caches
//!
//! Deletion is validated structurally (scope + shape + existence), never by
//! trusting the posted path, and always goes to the Trash.

use std::path::{Path, PathBuf};

use crate::lang::t;
use crate::trash;
use crate::util::{esc, home_dir, human, percent_decode};
use crate::walk::{dir_size, tildify};

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    NodeModules,
    LibraryCaches,
    UserCache,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "node_modules" => Some(Kind::NodeModules),
            "library_caches" => Some(Kind::LibraryCaches),
            "user_cache" => Some(Kind::UserCache),
            _ => None,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Kind::NodeModules => "node_modules",
            Kind::LibraryCaches => "library_caches",
            Kind::UserCache => "user_cache",
        }
    }
    /// The finding id this kind drills into (for the "expand" link in findings).
    pub fn for_finding(id: &str) -> Option<Kind> {
        match id {
            "node_modules_aggregate" => Some(Kind::NodeModules),
            "library_caches" => Some(Kind::LibraryCaches),
            "user_cache" => Some(Kind::UserCache),
            _ => None,
        }
    }
}

pub struct Item {
    pub path: PathBuf, // absolute
    pub size: u64,
}

fn development_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join("Development"))
}
fn caches_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join("Library/Caches"))
}
fn user_cache_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".cache"))
}
/// The container dir for a cache-style kind (its direct subdirs are the items).
fn container_for(kind: Kind) -> Option<PathBuf> {
    match kind {
        Kind::LibraryCaches => caches_dir(),
        Kind::UserCache => user_cache_dir(),
        Kind::NodeModules => None,
    }
}

/// Direct sub-directories of `dir` (used by the cache-style kinds). Discovery
/// only — sizing is done afterwards, in parallel.
fn list_subdirs(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let path = e.path();
            if path.is_dir() {
                out.push(path);
            }
        }
    }
}

/// Find every `node_modules` dir under `dir` (not recursing into one, to avoid
/// double-counting nested copies). Discovery only — sizing comes later.
fn collect_node_modules(dir: &Path, out: &mut Vec<PathBuf>) {
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for e in rd.flatten() {
        let path = e.path();
        let ft = match e.file_type() {
            Ok(f) => f,
            Err(_) => continue,
        };
        if !ft.is_dir() || ft.is_symlink() {
            continue;
        }
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name == "node_modules" {
            out.push(path); // don't recurse into it
        } else if name != ".git" {
            collect_node_modules(&path, out);
        }
    }
}

/// Size each discovered path concurrently — the `du`-style walk per item is the
/// slow part (a big ~/Development took ~10s serially), so fan it out across a
/// bounded set of threads. Mirrors the parallel `du` used by the `top` view.
fn sizes_parallel(paths: Vec<PathBuf>) -> Vec<Item> {
    if paths.len() < 2 {
        return paths
            .into_iter()
            .map(|p| Item {
                size: dir_size(&p),
                path: p,
            })
            .collect();
    }
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 8)
        .min(paths.len());
    let mut chunks: Vec<Vec<PathBuf>> = (0..workers).map(|_| Vec::new()).collect();
    for (i, p) in paths.into_iter().enumerate() {
        chunks[i % workers].push(p);
    }
    let handles: Vec<_> = chunks
        .into_iter()
        .map(|chunk| {
            std::thread::spawn(move || {
                chunk
                    .into_iter()
                    .map(|p| Item {
                        size: dir_size(&p),
                        path: p,
                    })
                    .collect::<Vec<Item>>()
            })
        })
        .collect();
    let mut out = Vec::new();
    for h in handles {
        if let Ok(mut v) = h.join() {
            out.append(&mut v);
        }
    }
    out
}

/// The sub-items for a kind, largest first (capped so the page stays reasonable).
pub fn items(kind: Kind) -> Vec<Item> {
    let mut paths = Vec::new();
    match kind {
        Kind::NodeModules => {
            if let Some(dev) = development_dir() {
                collect_node_modules(&dev, &mut paths);
            }
        }
        Kind::LibraryCaches | Kind::UserCache => {
            if let Some(dir) = container_for(kind) {
                list_subdirs(&dir, &mut paths);
            }
        }
    }
    let mut out = sizes_parallel(paths);
    out.sort_by_key(|i| std::cmp::Reverse(i.size));
    out.truncate(100);
    out
}

/// Structural check that a posted path is a legitimate sub-item of the kind —
/// scope + shape + existence — so the client can't ask us to trash anything else.
pub fn is_deletable(kind: Kind, path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    match kind {
        Kind::NodeModules => {
            path.file_name()
                .map(|n| n == "node_modules")
                .unwrap_or(false)
                && development_dir()
                    .map(|d| path.starts_with(d))
                    .unwrap_or(false)
        }
        Kind::LibraryCaches | Kind::UserCache => container_for(kind)
            .map(|c| path.parent() == Some(c.as_path()))
            .unwrap_or(false),
    }
}

/// Handle a POST from the drill-down picker: parse `kind` + the checked `del`
/// paths, move each valid one to the Trash, and return the redirect location.
/// Paths are validated with `is_deletable`, so a crafted path is ignored.
pub fn handle_post(body: &str) -> String {
    let field = |key: &str| -> Option<String> {
        body.split('&').find_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            (k == key).then(|| percent_decode(v))
        })
    };
    let kind = match field("kind").as_deref().and_then(Kind::parse) {
        Some(k) => k,
        None => return "/".to_string(),
    };
    for p in body.split('&').filter_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == "del").then(|| percent_decode(v))
    }) {
        let pb = PathBuf::from(&p);
        if is_deletable(kind, &pb) {
            let _ = trash::to_trash(&pb);
        }
    }
    format!("/breakdown?kind={}", kind.as_str())
}

fn intro(kind: Kind) -> &'static str {
    match kind {
        Kind::NodeModules => t(
            "Each project's node_modules. Safe to delete per-project — reinstall anytime with your package manager.",
            "プロジェクトごとの node_modules。個別に削除して安全（パッケージマネージャでいつでも再インストール可）。",
        ),
        Kind::LibraryCaches => t(
            "Each app's cache folder. Quit the app first; caches are regenerated on demand.",
            "アプリごとのキャッシュ。該当アプリを終了してから。キャッシュは必要時に再生成されます。",
        ),
        Kind::UserCache => t(
            "Each dev-tool's cache under ~/.cache (uv, huggingface, …). Re-downloaded on demand.",
            "~/.cache 配下の開発ツールのキャッシュ（uv, huggingface 等）。必要時に再取得されます。",
        ),
    }
}

/// The drill-down page body (sits inside the served shell).
pub fn page(kind: Kind) -> String {
    let items = items(kind);
    let heading = match kind {
        Kind::NodeModules => t(
            "<h2 style='font-size:18px;margin:2px 0 4px'>📦 node_modules — pick what to delete</h2>",
            "<h2 style='font-size:18px;margin:2px 0 4px'>📦 node_modules — 削除するものを選ぶ</h2>",
        ),
        Kind::LibraryCaches => t(
            "<h2 style='font-size:18px;margin:2px 0 4px'>🗂 ~/Library/Caches — pick what to delete</h2>",
            "<h2 style='font-size:18px;margin:2px 0 4px'>🗂 ~/Library/Caches — 削除するものを選ぶ</h2>",
        ),
        Kind::UserCache => t(
            "<h2 style='font-size:18px;margin:2px 0 4px'>🗂 ~/.cache — pick what to delete</h2>",
            "<h2 style='font-size:18px;margin:2px 0 4px'>🗂 ~/.cache — 削除するものを選ぶ</h2>",
        ),
    };
    let back = t("← Back to Scan", "← スキャンに戻る");
    let mut out = format!(
        "{heading}<div style='color:#57606a;font-size:13px;margin-bottom:6px'>{}</div>\
         <div style='margin-bottom:14px'><a href='/'>{back}</a></div>",
        intro(kind),
    );
    if items.is_empty() {
        out.push_str(&format!(
            "<p style='color:#57606a'>{}</p>",
            t("Nothing to show.", "表示する項目はありません。")
        ));
        return out;
    }
    let confirm = t(
        "Move the selected items to the Trash? (recoverable)",
        "選択した項目をゴミ箱へ移動しますか？（復元可能）",
    );
    let button = t("🗑 Move selected to Trash", "🗑 選択項目をゴミ箱へ");
    let note = t(
        "Checked items go to the Trash (recoverable) — never deleted outright.",
        "チェックした項目はゴミ箱へ（復元可）— 完全削除はしません。",
    );
    out.push_str(&format!(
        "<form method='post' action='/breakdown' onsubmit='return confirm(\"{confirm}\")'>\
         <input type='hidden' name='kind' value='{}'>",
        kind.as_str()
    ));
    for it in &items {
        out.push_str(&format!(
            "<label style='display:flex;justify-content:space-between;gap:12px;align-items:center;\
             padding:8px 10px;border:1px solid #d0d7de;border-radius:8px;margin:6px 0;background:#fff'>\
             <span><input type='checkbox' name='del' value='{}' style='margin-right:8px'>{}</span>\
             <span style='color:#57606a;white-space:nowrap'>{}</span></label>",
            esc(&it.path.to_string_lossy()),
            esc(&tildify(&it.path)),
            human(it.size),
        ));
    }
    out.push_str(&format!(
        "<button type='submit' style='margin-top:10px;background:#cf222e;color:#fff;border:0;\
         border-radius:8px;padding:9px 18px;font-size:14px;cursor:pointer'>{button}</button>\
         <div style='color:#57606a;font-size:12px;margin-top:6px'>{note}</div></form>"
    ));
    out
}
