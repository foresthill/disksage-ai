//! "What the AI receives" — the masked payload, exposed for display so the user
//! can confirm masking in the app (not just in the CLI/log files).
//!
//! `rows()` is the single source of truth for masking the per-finding metadata:
//! `ai::user_text` builds the actual request from these same rows, so what the
//! UI shows here is exactly what would be sent. Nothing is sent from this module.

use crate::lang::t;
use crate::mask;
use crate::scan::Finding;
use crate::util::{esc, human};

/// One finding as it would be sent: the real path (shown locally only), the
/// masked path/description that actually leave, and whether anything was masked.
pub struct Row {
    pub real_path: String,
    pub masked_path: String,
    pub masked_desc: String,
    pub anonymized: bool,
    pub pattern_id: &'static str,
    pub severity: &'static str,
    pub size: u64,
}

/// Mask every finding's path (registering its aliases) then its description with
/// the same aliases — identical to what `ai::user_text` sends.
pub fn rows(findings: &[Finding]) -> Vec<Row> {
    let mut aliases = mask::Aliases::new();
    findings
        .iter()
        .map(|f| {
            let masked_path = mask::mask_path(&f.path, &mut aliases);
            let masked_desc = mask::mask_text(&f.description, &aliases);
            Row {
                anonymized: masked_path != f.path,
                real_path: f.path.clone(),
                masked_path,
                masked_desc,
                pattern_id: f.id,
                severity: f.severity,
                size: f.size,
            }
        })
        .collect()
}

/// The /ai-preview page body: a table of real → masked, exactly what the AI gets.
pub fn page_html(findings: Option<&[Finding]>) -> String {
    let heading = t(
        "<h2 style='font-size:18px;margin:2px 0 4px'>🔒 What the AI receives</h2>",
        "<h2 style='font-size:18px;margin:2px 0 4px'>🔒 AI に渡す内容</h2>",
    );
    let intro = t(
        "Exactly the metadata that would be sent — file contents are never sent, and nothing is sent until you press \"Ask the AI\". User/app names are masked to &lt;dirN&gt;.",
        "送信されるメタデータそのものです。ファイル内容は送られず、「AIに判定してもらう」を押すまで何も送信しません。ユーザー/アプリ名は &lt;dirN&gt; にマスクされます。",
    );
    let back = t("← Back to Scan", "← スキャンに戻る");
    let mut out = format!(
        "{heading}<div style='color:#57606a;font-size:13px;margin-bottom:6px'>{intro}</div>\
         <div style='margin-bottom:14px'><a href='/'>{back}</a></div>"
    );
    let rows = match findings {
        Some(f) if !f.is_empty() => rows(f),
        _ => {
            out.push_str(&format!(
                "<p style='color:#57606a'>{}</p>",
                t(
                    "No findings yet — run a scan first.",
                    "まだ検出結果がありません。先にスキャンしてください。"
                )
            ));
            return out;
        }
    };
    let (h_real, h_masked, h_desc) = (
        t("Real path (local only)", "実パス（ローカルのみ）"),
        t("Sent as (masked)", "送信される値（マスク後）"),
        t("Description sent", "送信される説明"),
    );
    out.push_str(&format!(
        "<table style='width:100%;border-collapse:collapse;font-size:13px'>\
         <thead><tr style='text-align:left;color:#57606a'>\
         <th style='padding:6px 8px;border-bottom:1px solid #d0d7de'>{h_real}</th>\
         <th style='padding:6px 8px;border-bottom:1px solid #d0d7de'>{h_masked}</th>\
         <th style='padding:6px 8px;border-bottom:1px solid #d0d7de'>{h_desc}</th></tr></thead><tbody>"
    ));
    for r in &rows {
        let mark = if r.anonymized {
            " <span style='color:#1a7f37;font-size:11px'>✓ masked</span>"
        } else {
            ""
        };
        out.push_str(&format!(
            "<tr>\
             <td style='padding:6px 8px;border-bottom:1px solid #eaeef2;color:#8c959f'>{}</td>\
             <td style='padding:6px 8px;border-bottom:1px solid #eaeef2'>{}{mark}</td>\
             <td style='padding:6px 8px;border-bottom:1px solid #eaeef2'>{} <span style='color:#8c959f'>({})</span></td></tr>",
            esc(&r.real_path),
            esc(&r.masked_path),
            esc(&r.masked_desc),
            human(r.size),
        ));
    }
    out.push_str("</tbody></table>");
    out
}
