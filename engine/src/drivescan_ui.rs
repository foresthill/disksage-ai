//! Scan-page panel for the whole-drive MFT scan (see drivescan.rs).
//!
//! This is the answer to "the disk is 94% full but DiskSage found nothing": on
//! Windows the pattern/Top scans only see the user profile, so this panel offers
//! a one-click, administrator, whole-drive scan and shows where the space is.
//! The button only appears where the scan is supported (Windows); the result
//! renderers are platform-independent so they can be unit-tested anywhere.

use crate::drivescan::{supported, DriveScan, DriveScanError};
use crate::lang::{is_ja, t};
use crate::util::{esc, human};

/// The full-drive panel: heading + (Windows-only) button + any result. Returns
/// "" where the scan isn't supported and there's nothing to show, so it adds
/// nothing on macOS/Linux.
pub fn panel_html(result: Option<&Result<DriveScan, DriveScanError>>, running: bool) -> String {
    if !supported() && result.is_none() {
        return String::new();
    }
    let mut h =
        String::from("<div style='margin-top:26px;border-top:1px solid #d0d7de;padding-top:14px'>");
    h.push_str(t(
        "<h2 style='font-size:16px;margin:0 0 4px'>🔎 Whole-drive scan</h2>\
         <p style='color:#57606a;font-size:13px;margin:0 0 10px'>Reads the entire drive via the NTFS MFT \
         (like the Everything tool) — needed on Windows, where most of a full drive is outside your user folder. \
         Requires running DiskSage as administrator. Read-only, nothing is sent anywhere.</p>",
        "<h2 style='font-size:16px;margin:0 0 4px'>🔎 フルドライブスキャン</h2>\
         <p style='color:#57606a;font-size:13px;margin:0 0 10px'>ドライブ全体を NTFS MFT で読み取ります（Everything と同じ方式）。\
         ユーザーフォルダの外に容量の大半がある Windows で有効です。<b>管理者として実行</b>が必要。読み取り専用・どこにも送信しません。</p>",
    ));
    if supported() {
        h.push_str(&button(running));
    }
    if running {
        h.push_str(t(
            "<div style='margin:8px 0;padding:10px 12px;background:#fff8c5;border:1px solid #eac54f;\
             border-radius:8px;font-size:13px'>🔄 Scanning the whole drive…</div>",
            "<div style='margin:8px 0;padding:10px 12px;background:#fff8c5;border:1px solid #eac54f;\
             border-radius:8px;font-size:13px'>🔄 ドライブ全体をスキャン中…</div>",
        ));
    }
    match result {
        Some(Ok(scan)) => h.push_str(&results_html(scan)),
        Some(Err(e)) => h.push_str(&error_html(e)),
        None => {}
    }
    h.push_str("</div>");
    h
}

fn button(running: bool) -> String {
    let label = t("🔎 Scan the whole drive", "🔎 ドライブ全体をスキャン");
    let disabled = if running { " disabled" } else { "" };
    format!(
        "<form method='post' action='/drivescan'>\
         <button type='submit'{disabled} style='background:#0969da;color:#fff;border:0;border-radius:8px;\
         padding:9px 16px;font-size:14px;cursor:pointer'>{label}</button></form>"
    )
}

/// Biggest top-level items (bars) + a collapsible list of the biggest files.
fn results_html(s: &DriveScan) -> String {
    let max = s.folders.first().map(|e| e.bytes).unwrap_or(1).max(1);
    let mut rows = String::new();
    for e in &s.folders {
        let pct = (e.bytes as u128 * 100 / max as u128) as u64;
        rows.push_str(&format!(
            "<div style='margin:5px 0'>\
             <div style='display:flex;justify-content:space-between;font-size:13px'>\
             <span style='overflow-wrap:anywhere'>{}</span>\
             <span style='color:#57606a;white-space:nowrap;margin-left:8px'>{}</span></div>\
             <div style='background:#eaeef2;border-radius:4px;height:8px;overflow:hidden'>\
             <div style='width:{}%;height:8px;background:#0969da'></div></div></div>",
            esc(&e.name),
            human(e.bytes),
            pct
        ));
    }
    let mut files = String::new();
    for (p, b) in &s.files {
        files.push_str(&format!(
            "<li style='font-size:12px;color:#57606a;overflow-wrap:anywhere'>{} — {}</li>",
            human(*b),
            esc(p)
        ));
    }
    let summary = if is_ja() {
        format!(
            "<p style='font-size:13px;margin:6px 0'>{} — {} ファイル・合計 {}</p>",
            esc(&s.drive),
            s.file_count,
            human(s.total_bytes)
        )
    } else {
        format!(
            "<p style='font-size:13px;margin:6px 0'>{} — {} files · {} total</p>",
            esc(&s.drive),
            s.file_count,
            human(s.total_bytes)
        )
    };
    let files_label = t("Biggest individual files", "大きいファイル個別");
    format!(
        "{summary}{rows}\
         <details style='margin-top:10px'>\
         <summary style='cursor:pointer;font-size:13px'>{files_label}</summary>\
         <ul style='margin:6px 0 0;padding-left:18px'>{files}</ul></details>"
    )
}

fn error_html(e: &DriveScanError) -> String {
    let msg = match e {
        DriveScanError::NeedsAdmin => t(
            "Access denied. Quit DiskSage and relaunch it as administrator (right-click → Run as administrator) to scan the whole drive.",
            "アクセスが拒否されました。ドライブ全体をスキャンするには、DiskSage を一度終了し、管理者として実行してください（右クリック →「管理者として実行」）。",
        )
        .to_string(),
        DriveScanError::Unsupported => t(
            "The whole-drive scan is only available on Windows.",
            "フルドライブスキャンは Windows でのみ利用できます。",
        )
        .to_string(),
        DriveScanError::Failed(m) => {
            let lead = t("Scan failed:", "スキャンに失敗しました:");
            format!("{lead} {}", esc(m))
        }
    };
    format!(
        "<div style='margin:8px 0;padding:10px 12px;background:#ffebe9;border:1px solid #ffcecb;\
         border-radius:8px;font-size:13px'>⚠️ {msg}</div>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivescan::Entry;

    fn sample() -> DriveScan {
        DriveScan {
            drive: "C:".to_string(),
            folders: vec![
                Entry {
                    name: "Windows".to_string(),
                    bytes: 60 * 1024 * 1024 * 1024,
                },
                Entry {
                    name: "Users".to_string(),
                    bytes: 30 * 1024 * 1024 * 1024,
                },
            ],
            files: vec![("C:\\pagefile.sys".to_string(), 8 * 1024 * 1024 * 1024)],
            total_bytes: 98 * 1024 * 1024 * 1024,
            file_count: 1234,
        }
    }

    #[test]
    fn results_render_folders_and_files() {
        let ok = Ok(sample());
        let html = panel_html(Some(&ok), false);
        assert!(html.contains("Windows"));
        assert!(html.contains("Users"));
        assert!(html.contains("pagefile.sys"));
        // The biggest bucket gets a full-width bar.
        assert!(html.contains("width:100%"));
    }

    #[test]
    fn needs_admin_is_surfaced() {
        let err: Result<DriveScan, DriveScanError> = Err(DriveScanError::NeedsAdmin);
        let html = panel_html(Some(&err), false);
        assert!(html.contains("⚠️"));
    }
}
