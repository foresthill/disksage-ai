//! Rendering of the findings list for the served Scan page, including the
//! delete-to-Trash form (a checkbox only for the whitelisted, path-is-the-target
//! patterns).

use std::collections::HashMap;

use crate::ai::Judgment;
use crate::lang::t;
use crate::scan::Finding;
use crate::trash;
use crate::util::esc;

fn sev_rank(s: &str) -> u8 {
    match s {
        "high" => 0,
        "medium" => 1,
        "low" => 2,
        "info" => 3,
        "safe" => 4,
        _ => 5,
    }
}

fn sev_color(s: &str) -> &'static str {
    match s {
        "high" => "#d1242f",
        "medium" => "#bf8700",
        "low" => "#0969da",
        "safe" => "#1a7f37",
        _ => "#6e7781",
    }
}

/// Colour for an AI recommendation.
fn rec_color(r: &str) -> &'static str {
    match r {
        "safe_to_delete" => "#1a7f37",
        "archive_then_delete" => "#bf8700",
        "review_first" => "#0969da",
        "keep" => "#6e7781",
        _ => "#6e7781",
    }
}

/// The AI badge for one finding, if a judgment exists for it.
fn ai_badge(judgment: Option<&Judgment>) -> String {
    match judgment {
        None => String::new(),
        Some(j) => {
            let c = rec_color(&j.recommendation);
            format!(
                "<div style='margin-top:6px;font-size:13px'>\
                 <span style='background:{c};color:#fff;font-size:11px;padding:2px 8px;\
                 border-radius:10px'>🧠 {} · {}</span> {}</div>",
                esc(&j.recommendation),
                esc(&j.confidence),
                esc(&j.reasoning),
            )
        }
    }
}

/// Render the findings list. When `ai` is given, each finding shows its AI
/// judgment (matched by its original index).
pub fn findings_html(findings: &[Finding], ai: Option<&[Judgment]>) -> String {
    if findings.is_empty() {
        return format!(
            "<p style='color:#57606a'>{}</p>",
            t(
                "No findings above threshold. 🎉",
                "しきい値を超える項目はありません 🎉"
            )
        );
    }
    let ai_by_index: HashMap<usize, &Judgment> =
        ai.into_iter().flatten().map(|j| (j.index, j)).collect();
    // Keep original indices so AI judgments line up after sorting.
    let mut ordered: Vec<(usize, &Finding)> = findings.iter().enumerate().collect();
    ordered.sort_by(|a, b| {
        sev_rank(a.1.severity)
            .cmp(&sev_rank(b.1.severity))
            .then(b.1.size.cmp(&a.1.size))
    });
    let mut out = String::new();
    let mut any_deletable = false;
    for (idx, f) in ordered {
        let c = sev_color(f.severity);
        // A checkbox only for patterns whose path IS the thing to delete.
        let checkbox = if trash::is_deletable(f.id) {
            any_deletable = true;
            format!(
                "<input type='checkbox' name='del' value='{}' style='margin-right:8px'>",
                esc(&f.path)
            )
        } else {
            String::new()
        };
        out.push_str(&format!(
            "<div style='border-left:4px solid {c};background:#fff;border:1px solid #d0d7de;\
             border-radius:8px;padding:12px 14px;margin:10px 0'>\
             <div>{checkbox}<span style='background:{c};color:#fff;font-size:11px;padding:2px 8px;\
             border-radius:10px'>{}</span></div>\
             <div style='margin-top:6px'>{}</div>\
             <div style='color:#57606a;font-size:12px;margin-top:4px'>{}</div>\
             <div style='font-size:13px;margin-top:4px'>💡 {}</div>{}</div>",
            esc(&f.severity.to_uppercase()),
            esc(&f.description),
            esc(&f.path),
            esc(&f.action),
            ai_badge(ai_by_index.get(&idx).copied()),
        ));
    }
    if any_deletable {
        let confirm = t(
            "Move the selected items to the Trash? (recoverable)",
            "選択した項目をゴミ箱へ移動しますか？（復元可能）",
        );
        let button = t("🗑 Move selected to Trash", "🗑 選択項目をゴミ箱へ");
        let note = t(
            "Checked items go to the Trash (recoverable) — never deleted outright.",
            "チェックした項目はゴミ箱へ（復元可）— 完全削除はしません。",
        );
        format!(
            "<form method='post' action='/delete' onsubmit='return confirm(\"{confirm}\")'>\
             {out}<button type='submit' style='margin-top:8px;background:#cf222e;color:#fff;\
             border:0;border-radius:8px;padding:9px 18px;font-size:14px;cursor:pointer'>{button}</button>\
             <div style='color:#57606a;font-size:12px;margin-top:6px'>{note}</div></form>"
        )
    } else {
        out
    }
}
