//! Saved-report history for the served UI: lists the point-in-time snapshots the
//! CLI writes under `$DISKSAGE_HOME/scans` and serves a chosen one back, with a
//! whitelist check so a crafted `stamp` can't escape the scans directory.

use std::fs;
use std::path::PathBuf;

use crate::lang::t;
use crate::util::{esc, home_dir};

/// Where reports are saved: $DISKSAGE_HOME/scans (default ~/.disksage/scans).
pub fn scans_dir() -> PathBuf {
    let home = std::env::var_os("DISKSAGE_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|h| h.join(".disksage")))
        .unwrap_or_default();
    home.join("scans")
}

/// Saved report stamps (files named `<stamp>.html`), newest first.
fn saved_stamps() -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    if let Ok(rd) = fs::read_dir(scans_dir()) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if let Some(stem) = name.strip_suffix(".html") {
                v.push(stem.to_string());
            }
        }
    }
    v.sort();
    v.reverse();
    v
}

/// `2026-09-17_083000` → `2026-09-17 08:30:00` for display.
fn friendly(stamp: &str) -> String {
    if let Some((d, t)) = stamp.split_once('_') {
        if t.len() == 6 && t.bytes().all(|b| b.is_ascii_digit()) {
            return format!("{d} {}:{}:{}", &t[0..2], &t[2..4], &t[4..6]);
        }
    }
    stamp.to_string()
}

/// Value of a query parameter in a URL (no percent-decoding needed for stamps).
pub fn query_param<'a>(url: &'a str, key: &str) -> Option<&'a str> {
    let q = url.split_once('?')?.1;
    q.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        if k == key {
            Some(v)
        } else {
            None
        }
    })
}

/// The `/reports` history list (body HTML, sits inside the served shell).
pub fn reports_html() -> String {
    let stamps = saved_stamps();
    let mut out = format!(
        "<h2 style='font-size:18px;margin:2px 0 4px'>{}</h2>\
         <div style='color:#57606a;font-size:13px;margin-bottom:14px'>{}</div>",
        t("📁 Report history", "📁 レポート履歴"),
        t(
            "Each scan is a point-in-time snapshot.",
            "各スキャンはその時点の断面です。"
        ),
    );
    if stamps.is_empty() {
        out.push_str(&format!(
            "<p style='color:#57606a'>{}</p>",
            t(
                "No saved reports yet — run <code>disksage scan --html</code>.",
                "保存されたレポートはまだありません — <code>disksage scan --html</code> を実行してください。"
            )
        ));
        return out;
    }
    for (i, s) in stamps.iter().enumerate() {
        let tag = if i == 0 {
            t(
                " <span style='color:#1a7f37;font-size:12px'>· latest</span>",
                " <span style='color:#1a7f37;font-size:12px'>· 最新</span>",
            )
        } else {
            ""
        };
        out.push_str(&format!(
            "<a href='/report?stamp={}' style='display:flex;justify-content:space-between;\
             align-items:center;padding:12px 14px;border:1px solid #d0d7de;border-radius:8px;\
             margin:8px 0;text-decoration:none;color:#1f2328;background:#fff'>\
             <span>{}{}</span><span style='color:#57606a'>→</span></a>",
            esc(s),
            esc(&friendly(s)),
            tag,
        ));
    }
    out
}

/// A saved report's HTML, validated against the on-disk whitelist so an
/// arbitrary `stamp` can't escape the scans directory.
pub fn saved_report(stamp: &str) -> Option<String> {
    if !saved_stamps().iter().any(|s| s == stamp) {
        return None;
    }
    fs::read_to_string(scans_dir().join(format!("{stamp}.html"))).ok()
}
