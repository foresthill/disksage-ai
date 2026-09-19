//! Writes a self-contained HTML snapshot of a scan to `$DISKSAGE_HOME/scans`,
//! so the served /reports history works without the bash CLI. Each scan the
//! engine runs leaves a `<stamp>.html` behind, exactly where `reports.rs` looks.

use std::fs;

use chrono::Local;

use crate::df;
use crate::lang::t;
use crate::reports::scans_dir;
use crate::scan::Finding;
use crate::util::{esc, human};

fn sev_color(s: &str) -> &'static str {
    match s {
        "high" => "#d1242f",
        "medium" => "#bf8700",
        "low" => "#0969da",
        "safe" => "#1a7f37",
        _ => "#6e7781",
    }
}

fn disk_usage_html() -> String {
    let mut out = format!(
        "<h2>{}</h2>",
        t("Current Disk Usage", "現在のディスク使用量")
    );
    for (total, members) in df::containers().iter().rev() {
        let free = members.iter().map(|m| m.avail).max().unwrap_or(0);
        let used = total.saturating_sub(free);
        let pct = if *total > 0 { used * 100 / total } else { 0 };
        let raw = df::label_for(members);
        let label = if raw == "Startup disk" {
            t("Startup disk", "起動ディスク")
        } else {
            raw
        };
        out.push_str(&format!(
            "<p><b>{}</b> — {} / {} ({}%) · {} {}</p>",
            esc(label),
            human(used),
            human(*total),
            pct,
            t("free", "空き"),
            human(free),
        ));
    }
    out
}

fn findings_html(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return format!(
            "<p>{}</p>",
            t("No findings above threshold.", "しきい値を超える項目はありません。")
        );
    }
    let mut out = String::new();
    for f in findings {
        let c = sev_color(f.severity);
        out.push_str(&format!(
            "<div style='border-left:4px solid {c};padding:10px 14px;margin:10px 0;\
             background:#fff;border:1px solid #d0d7de;border-radius:8px'>\
             <span style='background:{c};color:#fff;font-size:11px;padding:2px 8px;\
             border-radius:10px'>{}</span><div style='margin-top:6px'>{}</div>\
             <div style='color:#57606a;font-size:12px'>{}</div>\
             <div style='font-size:13px;margin-top:4px'>💡 {}</div></div>",
            esc(&f.severity.to_uppercase()),
            esc(&f.description),
            esc(&f.path),
            esc(&f.action),
        ));
    }
    out
}

/// Render + save a snapshot; returns the stamp on success.
pub fn save(findings: &[Finding]) -> std::io::Result<String> {
    let stamp = Local::now().format("%Y-%m-%d_%H%M%S").to_string();
    let generated = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let heading = t("Findings", "検出結果");
    let doc = format!(
        "<!doctype html><html lang='{lang}'><head><meta charset='utf-8'>\
         <meta name='viewport' content='width=device-width, initial-scale=1'>\
         <title>DiskSage — {stamp}</title></head>\
         <body style='font-family:-apple-system,BlinkMacSystemFont,Helvetica,Arial,sans-serif;\
         max-width:880px;margin:24px auto;padding:0 16px;color:#1f2328'>\
         <h1>🩺 DiskSage</h1><p style='color:#57606a'>{gen_l}: {generated}</p>\
         {disk}<h2>🎯 {heading}</h2>{find}</body></html>",
        lang = t("en", "ja"),
        gen_l = t("Generated", "生成"),
        disk = disk_usage_html(),
        find = findings_html(findings),
    );
    let dir = scans_dir();
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(format!("{stamp}.html")), doc)?;
    Ok(stamp)
}
