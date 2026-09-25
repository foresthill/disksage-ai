//! The AI panel shown under the findings on the Scan page. Kept out of serve.rs
//! (which was over the file-size guideline) and decoupled from its ScanState.

use crate::ai::{self, Analysis};
use crate::lang::t;
use crate::util::esc;

fn ai_configured() -> bool {
    ai::resolve_provider().is_ok()
}

fn ai_button(rerun: bool) -> String {
    let label = if rerun {
        t("🧠 Re-run AI judgment", "🧠 AI再判定")
    } else {
        t("🧠 Ask the AI to judge", "🧠 AIに判定してもらう")
    };
    let note = t(
        "Sends masked metadata only (no file contents) to your configured provider.",
        "設定済みプロバイダにマスク済みメタデータのみ送信（ファイル内容は送りません）。",
    );
    format!(
        "<form method='post' action='/ai' style='margin-top:12px'>\
         <button type='submit' style='background:#8250df;color:#fff;border:0;border-radius:8px;\
         padding:9px 18px;font-size:14px;cursor:pointer'>{label}</button>\
         <div style='color:#57606a;font-size:12px;margin-top:6px'>{note}</div></form>"
    )
}

/// The AI area under the findings: running banner, error, token usage + re-run,
/// an opt-in button, or a note when no BYOK key is configured.
pub fn ai_controls(ai: Option<&Result<Analysis, String>>, ai_running: bool) -> String {
    if ai_running {
        return t(
            "<div style='margin-top:12px;padding:12px 14px;background:#ddf4ff;border:1px solid \
             #b6e3ff;border-radius:8px;font-size:14px'>🧠 Asking the AI…</div>",
            "<div style='margin-top:12px;padding:12px 14px;background:#ddf4ff;border:1px solid \
             #b6e3ff;border-radius:8px;font-size:14px'>🧠 AI が判定中…</div>",
        )
        .to_string();
    }
    if let Some(Err(e)) = ai {
        return format!(
            "<div style='margin-top:12px;padding:10px 14px;background:#ffebe9;border:1px solid \
             #ff8182;border-radius:8px;font-size:13px'>⚠️ {} {}</div>{}",
            t("AI request failed:", "AI判定に失敗:"),
            esc(e),
            ai_button(true),
        );
    }
    // A completed analysis: show the token usage (factual, from the API) + re-run.
    if let Some(Ok(a)) = ai {
        let usage = a
            .usage
            .as_ref()
            .map(|u| {
                format!(
                    "<div style='margin-top:12px;font-size:13px;color:#57606a'>🪙 {}: {} · {}: {}</div>",
                    t("input tokens", "入力トークン"),
                    u.input_tokens,
                    t("output tokens", "出力トークン"),
                    u.output_tokens,
                )
            })
            .unwrap_or_default();
        return format!("{usage}{}", ai_button(true));
    }
    if !ai_configured() {
        return format!(
            "<div style='margin-top:12px;color:#57606a;font-size:13px'>{}</div>",
            t(
                "AI judgment is available when a BYOK key is set (ANTHROPIC_API_KEY or OPENROUTER_API_KEY).",
                "AI判定は BYOK キー設定時に使えます（ANTHROPIC_API_KEY または OPENROUTER_API_KEY）。",
            )
        );
    }
    ai_button(false)
}
