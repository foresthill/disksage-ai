//! Rendering of the findings list for the served Scan page, including the
//! delete-to-Trash form (a checkbox only for the whitelisted, path-is-the-target
//! patterns).

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

pub fn findings_html(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return format!(
            "<p style='color:#57606a'>{}</p>",
            t("No findings above threshold. 🎉", "しきい値を超える項目はありません 🎉")
        );
    }
    let mut ordered: Vec<&Finding> = findings.iter().collect();
    ordered.sort_by(|a, b| {
        sev_rank(a.severity)
            .cmp(&sev_rank(b.severity))
            .then(b.size.cmp(&a.size))
    });
    let mut out = String::new();
    let mut any_deletable = false;
    for f in ordered {
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
             <div style='font-size:13px;margin-top:4px'>💡 {}</div></div>",
            esc(&f.severity.to_uppercase()),
            esc(&f.description),
            esc(&f.path),
            esc(&f.action),
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
