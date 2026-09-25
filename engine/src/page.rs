//! Shared page fragments for the served UI (kept out of serve.rs to stay under
//! the file-size guideline).

use crate::df;
use crate::lang::{is_ja, t};
use crate::util::{esc, human};

/// The left sidebar nav. `active` is the current section key (scan/reports/settings).
fn sidebar(active: &str) -> String {
    let item = |href: &str, icon: &str, label: &str, key: &str| {
        let bg = if key == active { "background:#30363d;" } else { "" };
        format!(
            "<a href='{href}' style='display:block;padding:10px 12px;border-radius:8px;\
             color:#fff;text-decoration:none;margin:2px 0;font-size:14px;{bg}'>{icon} {label}</a>"
        )
    };
    format!(
        "<nav style='position:fixed;left:0;top:0;bottom:0;width:200px;background:#1f2328;\
         color:#fff;padding:18px 14px;box-sizing:border-box;z-index:20;overflow:auto'>\
         <div style='font-weight:700;font-size:17px;margin:2px 0 18px'>🩺 DiskSage</div>{}{}{}</nav>",
        item("/", "🔍", t("Scan", "スキャン"), "scan"),
        item("/reports", "📁", t("Reports", "レポート"), "reports"),
        item("/settings", "⚙️", t("Settings", "設定"), "settings"),
    )
}

/// The full HTML document: sidebar + a centered content card. `refresh` injects a
/// 2s meta-refresh (used while a scan/AI call is in flight).
pub fn shell(active: &str, title: &str, body: &str, refresh: bool) -> String {
    let meta = if refresh {
        "<meta http-equiv='refresh' content='2'>"
    } else {
        ""
    };
    format!(
        "<!doctype html><html lang='{lang}'><head><meta charset='utf-8'>{meta}\
         <meta name='viewport' content='width=device-width, initial-scale=1'>\
         <title>DiskSage — {title}</title>\
         <style>body{{padding-left:200px}}@media(max-width:640px){{body{{padding-left:0}}}}</style></head>\
         <body style='font-family:-apple-system,BlinkMacSystemFont,Helvetica,Arial,sans-serif;\
         margin:0;background:#f6f8fa;color:#1f2328'>{sidebar}\
         <div style='max-width:880px;margin:24px auto;background:#fff;border:1px solid #d0d7de;\
         border-radius:12px;padding:28px 32px'>{body}</div></body></html>",
        lang = t("en", "ja"),
        sidebar = sidebar(active),
    )
}

/// The disk-usage overview: one bar per APFS container + a snapshot warning.
pub fn overview_html() -> String {
    let heading = t(
        "<h2 style='font-size:16px;margin:6px 0 14px'>📊 Current Disk Usage</h2>",
        "<h2 style='font-size:16px;margin:6px 0 14px'>📊 現在のディスク使用量</h2>",
    );
    let mut parts = String::from(heading);
    for (total, members) in df::containers().iter().rev() {
        let free = members.iter().map(|m| m.avail).max().unwrap_or(0);
        let used = total.saturating_sub(free);
        let pct = if *total > 0 { used * 100 / total } else { 0 };
        let color = if pct >= 90 {
            "#d1242f"
        } else if pct >= 75 {
            "#bf8700"
        } else {
            "#1a7f37"
        };
        let raw = df::label_for(members);
        let label = if raw == "Startup disk" {
            t("Startup disk", "起動ディスク")
        } else {
            raw
        };
        parts.push_str(&format!(
            "<div style='margin:10px 0'><div style='font-weight:600'>{}</div>\
             <div style='display:flex;align-items:center;gap:10px'>\
             <span style='flex:1;height:12px;background:#e6e6e6;border-radius:6px;overflow:hidden'>\
             <span style='display:block;height:100%;width:{}%;background:{color}'></span></span>\
             <span style='white-space:nowrap;color:#57606a;font-size:13px'>{} / {} ({}%) · {} {}</span>\
             </div></div>",
            esc(label),
            pct.min(100),
            human(used),
            human(*total),
            pct,
            t("free", "空き"),
            human(free),
        ));
    }
    if let Some(n) = df::snapshot_count() {
        if n > 0 {
            let msg = if is_ja() {
                format!("ローカルスナップショット: {n} 個 — 空きを保持している可能性（削除済みファイルを生かし続けます）。")
            } else {
                format!("Local snapshots: {n} — these can hold space (each keeps recently-deleted files alive).")
            };
            parts.push_str(&format!(
                "<div style='margin-top:14px;padding:12px 14px;background:#fff8c5;border:1px solid \
                 #eac54f;border-radius:8px;font-size:13px'>{msg}</div>"
            ));
        }
    }
    parts
}
