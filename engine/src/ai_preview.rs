//! "What the AI receives" — the masked payload, exposed for display so the user
//! can confirm masking in the app (not just in the CLI/log files).
//!
//! `rows()` is the single source of truth for masking the per-finding metadata:
//! `ai::user_text` builds the actual request from these same rows, so what the
//! UI shows here is exactly what would be sent. Nothing is sent from this module.

use std::collections::BTreeMap;

use serde_json::{json, Value};

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
    /// The user's own note for this path, masked with the same aliases. `None`
    /// when the path has no note. Free text, so masking is best-effort (it scrubs
    /// names already anonymized in the paths); the preview shows it so the user can
    /// redact before sending.
    pub masked_note: Option<String>,
    pub anonymized: bool,
    pub pattern_id: &'static str,
    pub severity: &'static str,
    pub size: u64,
}

/// Mask every finding's path (registering its aliases), then its description and
/// the user's note with the same aliases — identical to what `ai::user_text`
/// sends. Reads the user's notes once from disk.
pub fn rows(findings: &[Finding]) -> Vec<Row> {
    rows_with_notes(findings, &crate::notes::all())
}

/// The masking core, with the notes map injected so it can be unit-tested without
/// touching the real `notes.json`.
fn rows_with_notes(findings: &[Finding], notes: &BTreeMap<String, String>) -> Vec<Row> {
    let mut aliases = mask::Aliases::new();
    findings
        .iter()
        .map(|f| {
            let masked_path = mask::mask_path(&f.path, &mut aliases);
            let masked_desc = mask::mask_text(&f.description, &aliases);
            // The note is masked with the aliases built so far (same as the desc),
            // so a name already anonymized in a path is scrubbed here too.
            let masked_note =
                crate::notes::for_path(notes, &f.path).map(|n| mask::mask_text(n, &aliases));
            Row {
                anonymized: masked_path != f.path,
                real_path: f.path.clone(),
                masked_path,
                masked_desc,
                masked_note,
                pattern_id: f.id,
                severity: f.severity,
                size: f.size,
            }
        })
        .collect()
}

/// The findings as the JSON array sent to the model: masked path, size, severity,
/// masked description, and the user's own masked note when present (`user_note`).
/// The single builder shared by the judgment prompt (`ai::user_text`) and the chat
/// prompt (`chat::system_prompt`), so both send exactly what `/ai-preview` shows.
pub fn finding_items(findings: &[Finding]) -> Vec<Value> {
    items_from_rows(&rows(findings))
}

/// Build the JSON items from already-masked rows (pure; testable without disk).
fn items_from_rows(rows: &[Row]) -> Vec<Value> {
    rows.iter()
        .enumerate()
        .map(|(i, r)| {
            let mut item = json!({
                "index": i,
                "pattern_id": r.pattern_id,
                "path": r.masked_path,
                "size_bytes": r.size,
                "heuristic_severity": r.severity,
                "description": r.masked_desc,
            });
            if let Some(note) = &r.masked_note {
                // The user's own context about this item (pull-only: read at
                // judgment time, never stored back or used to train).
                item["user_note"] = Value::String(note.clone());
            }
            item
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
        "Exactly the metadata that would be sent — file contents are never sent, and nothing is sent until you press \"Ask the AI\". User/app names are masked to &lt;dirN&gt;. Your own notes are included as context (also masked); since a note is free text, review it below and edit the note if anything in it should not be sent.",
        "送信されるメタデータそのものです。ファイル内容は送られず、「AIに判定してもらう」を押すまで何も送信しません。ユーザー/アプリ名は &lt;dirN&gt; にマスクされます。あなたのメモも文脈として同梱します（同様にマスク）。メモは自由文のため、下の内容を確認し、送りたくない記述があればメモを編集してください。",
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
    let note_label = t("note", "メモ");
    for r in &rows {
        let mark = if r.anonymized {
            " <span style='color:#1a7f37;font-size:11px'>✓ masked</span>"
        } else {
            ""
        };
        // Show the user's note under the description, exactly as it would be sent,
        // so they can spot anything to redact before pressing "Ask the AI".
        let note = match &r.masked_note {
            Some(n) => format!(
                "<div style='margin-top:4px;color:#57606a'>📝 {note_label}: {}</div>",
                esc(n)
            ),
            None => String::new(),
        };
        out.push_str(&format!(
            "<tr>\
             <td style='padding:6px 8px;border-bottom:1px solid #eaeef2;color:#8c959f'>{}</td>\
             <td style='padding:6px 8px;border-bottom:1px solid #eaeef2'>{}{mark}</td>\
             <td style='padding:6px 8px;border-bottom:1px solid #eaeef2'>{} <span style='color:#8c959f'>({})</span>{note}</td></tr>",
            esc(&r.real_path),
            esc(&r.masked_path),
            esc(&r.masked_desc),
            human(r.size),
        ));
    }
    out.push_str("</tbody></table>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(id: &'static str, path: &str) -> Finding {
        Finding {
            id,
            path: path.to_string(),
            size: 1024,
            severity: "low",
            description: "example".to_string(),
            action: "example".to_string(),
        }
    }

    #[test]
    fn note_is_masked_with_the_same_aliases_as_the_path() {
        let f = vec![finding(
            "node_modules_aggregate",
            "~/Development/SecretProj/node_modules",
        )];
        let mut notes = BTreeMap::new();
        notes.insert(
            "~/Development/SecretProj/node_modules".to_string(),
            "SecretProj is an old client project — keep for now".to_string(),
        );
        let rows = rows_with_notes(&f, &notes);
        let note = rows[0].masked_note.as_deref().expect("note present");
        // The name anonymized in the path (<dir1>) is scrubbed from the note too.
        assert!(note.contains("<dir1>"), "note not masked: {note}");
        assert!(
            !note.contains("SecretProj"),
            "raw user name leaked in note: {note}"
        );
    }

    #[test]
    fn no_note_means_none_and_no_user_note_field() {
        let f = vec![finding("ollama_models", "~/.ollama/models")];
        let rows = rows_with_notes(&f, &BTreeMap::new());
        assert!(rows[0].masked_note.is_none());
        // And the JSON item carries no user_note key when there's no note.
        let items = items_from_rows(&rows);
        assert!(items[0].get("user_note").is_none());
    }

    #[test]
    fn user_note_field_is_present_and_masked_when_a_note_exists() {
        let f = vec![finding(
            "node_modules_aggregate",
            "~/Development/SecretProj/node_modules",
        )];
        let mut notes = BTreeMap::new();
        notes.insert(
            "~/Development/SecretProj/node_modules".to_string(),
            "SecretProj: paused client work".to_string(),
        );
        let items = items_from_rows(&rows_with_notes(&f, &notes));
        let note = items[0]["user_note"].as_str().expect("user_note present");
        assert!(note.contains("<dir1>"));
        assert!(!note.contains("SecretProj"));
    }
}
