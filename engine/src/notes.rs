//! User-authored notes for a finding, keyed by its path.
//!
//! These are the user's own words about an item — why they keep it, what they
//! found ("opened it — it was last year's export"), the cause of something. They
//! are a separate layer from the auto-detected report: DiskSage stores them
//! verbatim and never rewrites or auto-generates them (no churn, no "learning
//! loop"). At AI-judgment time the note for a path is handed to the model as
//! context (see `ai_preview::finding_items`) — pull-only, read fresh each time,
//! never stored back or used to train. It is masked with the same aliases as the
//! paths and shown in `/ai-preview` before anything is sent, so the user can
//! redact it first.
//!
//! Storage: `$DISKSAGE_HOME/notes.json`, a flat `{path: text}` map. The path is
//! used only as an opaque key — notes never touch the filesystem — so, unlike
//! delete or reveal, no path whitelist is needed here.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::lang::t;
use crate::util::{esc, home_dir, percent_decode, percent_encode};

/// `$DISKSAGE_HOME/notes.json` (default `~/.disksage/notes.json`).
fn notes_path() -> PathBuf {
    let home = std::env::var_os("DISKSAGE_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|h| h.join(".disksage")))
        .unwrap_or_default();
    home.join("notes.json")
}

/// The whole {path: text} map (empty when the file is missing or unreadable).
fn load() -> BTreeMap<String, String> {
    fs::read_to_string(notes_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Persist the map (pretty JSON). Best-effort; the caller ignores the result.
fn store(map: &BTreeMap<String, String>) -> std::io::Result<()> {
    let path = notes_path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(map).unwrap_or_else(|_| "{}".to_string());
    fs::write(path, json)
}

/// The note for a path within a loaded map, if it is present and non-blank.
fn note_of<'a>(map: &'a BTreeMap<String, String>, path: &str) -> Option<&'a str> {
    map.get(path)
        .map(|s| s.as_str())
        .filter(|s| !s.trim().is_empty())
}

/// Set (non-blank) or clear (blank) the note for a path. Best-effort.
fn set(path: &str, text: &str) {
    let mut map = load();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        map.remove(path);
    } else {
        map.insert(path.to_string(), trimmed.to_string());
    }
    let _ = store(&map);
}

/// Load the whole map once (callers that render many cards read it a single time,
/// not once per finding).
pub fn all() -> BTreeMap<String, String> {
    load()
}

/// The note for a path within an already-loaded map — a public lookup for callers
/// that hold the map (e.g. the AI preview, which masks the note before sending).
/// `None` when the path has no note or only a blank one.
pub fn for_path<'a>(map: &'a BTreeMap<String, String>, path: &str) -> Option<&'a str> {
    note_of(map, path)
}

/// The note block shown on a finding card: the saved note (if any) plus a link to
/// the editor. A plain link — not a nested `<form>` — because the finding cards
/// live inside the delete form. `map` is the once-loaded map from `all()`.
pub fn card_html(map: &BTreeMap<String, String>, path: &str) -> String {
    let href = format!("/note?path={}", percent_encode(path));
    match note_of(map, path) {
        Some(note) => format!(
            "<div style='margin-top:8px;background:#f6f8fa;border:1px solid #d0d7de;\
             border-radius:6px;padding:8px 10px;font-size:13px;white-space:pre-wrap'>📝 {}\
             <div style='margin-top:4px'><a href='{href}' style='font-size:12px'>{}</a></div></div>",
            esc(note),
            t("Edit note", "メモを編集"),
        ),
        None => format!(
            "<div style='margin-top:6px'><a href='{href}' style='font-size:13px'>{}</a></div>",
            t("📝 Add a note", "📝 メモを書く"),
        ),
    }
}

/// The note editor page body (sits inside the served shell).
pub fn page(path: &str) -> String {
    let map = load();
    let current = note_of(&map, path).unwrap_or("");
    let heading = t(
        "<h2 style='font-size:18px;margin:2px 0 4px'>📝 Note</h2>",
        "<h2 style='font-size:18px;margin:2px 0 4px'>📝 メモ</h2>",
    );
    let intro = t(
        "Your own note about this item — why you keep it, what you found, the cause. \
         DiskSage stores it as-is and never rewrites it.",
        "この項目についてのあなた自身のメモ — 残す理由、原因、「開いたら○○だった」など。\
         DiskSage はそのまま保存し、書き換えません。",
    );
    let back = t("← Back to Scan", "← スキャンに戻る");
    let save_label = t("💾 Save", "💾 保存");
    let clear_hint = t(
        "Leave it empty and save to remove the note.",
        "空にして保存すると削除されます。",
    );
    let privacy = t(
        "This note is NOT sent anywhere automatically. (A later option can hand it to \
         the AI as context when you ask for a judgment.)",
        "このメモは自動でどこにも送信されません。（将来、AI 判定を頼んだときに文脈として\
         渡すオプションを追加予定）",
    );
    format!(
        "{heading}\
         <div style='color:#57606a;font-size:12px;margin-bottom:6px;word-break:break-all'>📂 {p}</div>\
         <div style='color:#57606a;font-size:13px;margin-bottom:4px'>{intro}</div>\
         <div style='margin-bottom:12px'><a href='/'>{back}</a></div>\
         <form method='post' action='/note'>\
         <input type='hidden' name='path' value='{p}'>\
         <textarea name='text' rows='6' style='width:100%;max-width:640px;box-sizing:border-box;\
         font-size:14px;padding:8px;border:1px solid #d0d7de;border-radius:8px'>{cur}</textarea>\
         <div style='margin-top:10px'>\
         <button type='submit' style='background:#1f883d;color:#fff;border:0;border-radius:8px;\
         padding:9px 18px;font-size:14px;cursor:pointer'>{save_label}</button></div>\
         <div style='color:#57606a;font-size:12px;margin-top:6px'>{clear_hint}</div>\
         <div style='color:#57606a;font-size:12px;margin-top:10px'>🔒 {privacy}</div>\
         </form>",
        p = esc(path),
        cur = esc(current),
    )
}

/// Save a posted note (`path` + `text`), then redirect back to the Scan page.
pub fn handle_post(body: &str) -> String {
    let field = |key: &str| -> Option<String> {
        body.split('&').find_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            (k == key).then(|| percent_decode(v))
        })
    };
    if let Some(path) = field("path") {
        set(&path, &field("text").unwrap_or_default());
    }
    "/".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_of_ignores_blank_entries() {
        let mut m = BTreeMap::new();
        m.insert("a".to_string(), "  ".to_string());
        m.insert("b".to_string(), "kept on purpose".to_string());
        assert_eq!(note_of(&m, "a"), None);
        assert_eq!(note_of(&m, "b"), Some("kept on purpose"));
        assert_eq!(note_of(&m, "missing"), None);
    }

    #[test]
    fn handle_post_without_path_is_a_noop_redirect() {
        // No `path` field → set() is never called (no file write), and it still
        // redirects to the Scan page. Avoids touching the real notes.json in CI.
        assert_eq!(handle_post("text=hi"), "/");
    }
}
