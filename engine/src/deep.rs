//! The Full ("じっくり") scan: biggest folders + recently-grown files, and the UI
//! block for them. Kept out of serve.rs so that file stays small.
//!
//! Quick scan (the default) shows the known-hot-spot findings fast; Full scan
//! adds these two deeper, slower views for when you want to dig.

use crate::lang::{is_ja, t};
use crate::util::{esc, human};
use crate::{flow, top};

pub struct DeepScan {
    pub top: Vec<(String, u64)>,
    pub flow: Vec<(u64, String)>,
}

/// Run the deep passes (slow: walks all of $HOME). Call off the request thread.
pub fn run() -> DeepScan {
    DeepScan {
        top: top::biggest_folders(20),
        flow: flow::recent_large(20),
    }
}

fn heading() -> &'static str {
    t(
        "<h2 style='font-size:16px;margin:26px 0 8px'>🔬 Full scan (deep dive)</h2>",
        "<h2 style='font-size:16px;margin:26px 0 8px'>🔬 じっくりスキャン（深掘り）</h2>",
    )
}

fn button(rerun: bool) -> String {
    let label = if rerun {
        t("🔬 Re-run Full scan", "🔬 じっくり再スキャン")
    } else {
        t("🔬 Run Full scan (じっくり)", "🔬 じっくりスキャン")
    };
    let note = t(
        "Walks your whole home for the biggest folders and recently-grown files — slower, for when you want to dig.",
        "ホーム全体を走査して「大きいフォルダ」と「最近増えたファイル」を洗い出します（時間はかかります）。",
    );
    format!(
        "<form method='post' action='/fullscan' style='margin-top:8px'>\
         <button type='submit' style='background:#1a7f37;color:#fff;border:0;border-radius:8px;\
         padding:9px 18px;font-size:14px;cursor:pointer'>{label}</button>\
         <div style='color:#57606a;font-size:12px;margin-top:6px'>{note}</div></form>"
    )
}

fn folders_html(rows: &[(String, u64)]) -> String {
    let title = t(
        "<h3 style='font-size:14px;margin:16px 0 6px'>📁 Biggest folders</h3>",
        "<h3 style='font-size:14px;margin:16px 0 6px'>📁 大きいフォルダ</h3>",
    );
    if rows.is_empty() {
        return format!("{title}<p style='color:#57606a;font-size:13px'>—</p>");
    }
    let mut out = String::from(title);
    out.push_str("<div style='font-size:13px'>");
    for (path, size) in rows {
        out.push_str(&format!(
            "<div style='display:flex;justify-content:space-between;gap:12px;padding:4px 0;\
             border-bottom:1px solid #eaeef2'><span style='color:#1f2328'>{}</span>\
             <span style='color:#57606a;white-space:nowrap'>{}</span></div>",
            esc(path),
            human(*size),
        ));
    }
    out.push_str("</div>");
    out
}

fn recent_html(rows: &[(u64, String)]) -> String {
    let title = t(
        "<h3 style='font-size:14px;margin:18px 0 6px'>📈 Recently grown (&gt;500MB, &lt;30d)</h3>",
        "<h3 style='font-size:14px;margin:18px 0 6px'>📈 最近増えたファイル（500MB超・30日以内）</h3>",
    );
    if rows.is_empty() {
        return format!("{title}<p style='color:#57606a;font-size:13px'>—</p>");
    }
    let archive = t("← back up to an external drive", "← 外付けへ退避");
    let mut out = String::from(title);
    out.push_str("<div style='font-size:13px'>");
    for (size, path) in rows {
        let media = if flow::is_media(path) {
            format!("<span style='color:#bf8700'> {archive}</span>")
        } else {
            String::new()
        };
        out.push_str(&format!(
            "<div style='display:flex;justify-content:space-between;gap:12px;padding:4px 0;\
             border-bottom:1px solid #eaeef2'><span style='color:#1f2328'>{}{}</span>\
             <span style='color:#57606a;white-space:nowrap'>{}</span></div>",
            esc(path),
            media,
            human(*size),
        ));
    }
    out.push_str("</div>");
    let note = if is_ja() {
        "<div style='color:#57606a;font-size:12px;margin-top:6px'>キャッシュは削除OK。でも写真/動画/制作ファイルは二度と戻せません→ USB/外付けへコピーしてから削除を。</div>"
    } else {
        "<div style='color:#57606a;font-size:12px;margin-top:6px'>Caches are fine to delete, but photos/videos/project files can't be regenerated — copy them to an external drive first.</div>"
    };
    out.push_str(note);
    out
}

/// The Full-scan block: a trigger button, a running banner, or the two result
/// sections (with a re-run button).
pub fn section_html(deep: Option<&DeepScan>, running: bool) -> String {
    let mut out = String::from(heading());
    if running {
        out.push_str(t(
            "<div style='padding:12px 14px;background:#ddf4ff;border:1px solid #b6e3ff;\
             border-radius:8px;font-size:14px'>🔬 Digging through your home… this can take a while.</div>",
            "<div style='padding:12px 14px;background:#ddf4ff;border:1px solid #b6e3ff;\
             border-radius:8px;font-size:14px'>🔬 ホーム全体を深掘り中… 少し時間がかかります。</div>",
        ));
        return out;
    }
    match deep {
        None => out.push_str(&button(false)),
        Some(d) => {
            out.push_str(&folders_html(&d.top));
            out.push_str(&recent_html(&d.flow));
            out.push_str(&button(true));
        }
    }
    out
}
