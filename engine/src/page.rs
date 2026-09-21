//! Shared page fragments for the served UI (kept out of serve.rs to stay under
//! the file-size guideline).

use crate::df;
use crate::lang::{is_ja, t};
use crate::util::{esc, human};

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
