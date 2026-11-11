//! Follow-up chat about the findings. After the one-shot judgment, the user can
//! ask the AI questions ("is the ms-playwright cache safe to delete?", "what is
//! dotslash?") in a multi-turn conversation.
//!
//! Privacy is unchanged from the judgment path: the findings context is built from
//! `ai_preview::rows` (the single masking source) — masked paths, sizes and
//! heuristic descriptions only, never file contents. The user's own typed messages
//! are sent to the provider as-is (the UI says so). DiskSage never deletes anything
//! from a chat reply: deletion still goes through the checkbox + Trash flow.
//!
//! This module holds the shared types, the prompt and the dispatch; the per-provider
//! transport lives in `chat_backends`, and the panel UI in `chat_ui`.

use serde_json::{json, Value};

use crate::ai::{self, Usage};
use crate::scan::Finding;

/// One conversation turn. `role` is "user" or "assistant".
#[derive(Clone)]
pub struct ChatTurn {
    pub role: String,
    pub content: String,
}

/// An assistant reply plus the call's token usage / cost (when the backend reports
/// them), mirroring `ai::Analysis` so the UI can show the same footer.
#[derive(Debug)]
pub struct ChatReply {
    pub text: String,
    pub usage: Option<Usage>,
    pub cost_usd: Option<f64>,
}

/// The conversational system prompt: advisor framing + the masked findings as
/// context. Different from the judgment prompt (free text, no JSON schema).
fn system_prompt(findings: &[Finding], lang_ja: bool) -> String {
    let items: Vec<Value> = crate::ai_preview::rows(findings)
        .iter()
        .enumerate()
        .map(|(i, r)| {
            json!({
                "index": i,
                "pattern_id": r.pattern_id,
                "path": r.masked_path,
                "size_bytes": r.size,
                "heuristic_severity": r.severity,
                "description": r.masked_desc,
            })
        })
        .collect();
    let mut s = format!(
        "You are a disk-cleanup advisor for DiskSage, a tool that NEVER deletes anything \
         automatically — it only suggests, and the user confirms each deletion (files go to the \
         Trash). The user is looking at these disk-space findings, given to you as METADATA ONLY \
         (masked paths, sizes, heuristic descriptions — you never see file contents):\n{}\n\n\
         Answer the user's follow-up questions concisely, for a technical user. Reason only from \
         this metadata; if you lack the data to be sure, say so plainly. Be conservative about \
         anything that could be irreplaceable user data. Never tell the user to run destructive \
         shell commands — DiskSage handles deletion safely through its own Trash flow.",
        serde_json::to_string_pretty(&items).unwrap_or_default()
    );
    if lang_ja {
        s.push_str(" Reply in Japanese.");
    }
    s
}

/// Send the conversation and return the assistant's reply. Dispatches to the same
/// backend the judgment uses (Ollama → Claude CLI → BYOK HTTP). Audit logging
/// (`--ai-log` / toggle) records the request and raw response when enabled.
pub fn reply(
    history: &[ChatTurn],
    findings: &[Finding],
    lang_ja: bool,
    audit_log: bool,
) -> Result<ChatReply, String> {
    let system = system_prompt(findings, lang_ja);
    if ai::use_ollama() {
        crate::chat_backends::ollama_chat(&system, history, findings, audit_log)
    } else if ai::use_claude_cli() {
        crate::chat_backends::claude_chat(&system, history, findings, audit_log)
    } else {
        crate::chat_backends::http_chat(&system, history, findings, audit_log)
    }
}

/// Turn a reply (or error) into the assistant turn's text, appending a token /
/// cost footer when the backend reported them. Kept here so serve.rs stays a thin
/// router.
pub fn format_reply(reply: Result<ChatReply, String>, lang_ja: bool) -> String {
    match reply {
        Ok(r) => {
            let mut c = r.text;
            if let Some(u) = r.usage {
                c.push_str(&format!(
                    "\n\n🪙 {} {} / {} {}",
                    t2(lang_ja, "in", "入力"),
                    u.input_tokens,
                    t2(lang_ja, "out", "出力"),
                    u.output_tokens,
                ));
            }
            if let Some(cost) = r.cost_usd {
                c.push_str(&format!(" · 💵 ${cost:.4}"));
            }
            c
        }
        Err(e) => format!("⚠️ {e}"),
    }
}

/// Pick a string by language. `t()` reads the global locale; the reply is formatted
/// on a worker thread with the request's `lang_ja` passed in explicitly.
fn t2(ja: bool, en: &'static str, jp: &'static str) -> &'static str {
    if ja {
        jp
    } else {
        en
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::Finding;

    fn finding(id: &'static str, path: &str) -> Finding {
        Finding {
            id,
            path: path.to_string(),
            size: 2048,
            severity: "low",
            description: "example".to_string(),
            action: "example".to_string(),
        }
    }

    #[test]
    fn system_prompt_masks_paths() {
        let f = vec![finding(
            "node_modules_aggregate",
            "~/Development/SecretProj/node_modules",
        )];
        let s = system_prompt(&f, true);
        assert!(s.contains("<dir1>"), "user dir anonymized");
        assert!(!s.contains("SecretProj"), "raw user name must NOT appear");
        assert!(s.contains("Reply in Japanese."));
    }

    #[test]
    fn format_reply_appends_usage_and_errors() {
        let ok = ChatReply {
            text: "caches are safe".into(),
            usage: Some(Usage {
                input_tokens: 10,
                output_tokens: 5,
            }),
            cost_usd: Some(0.25),
        };
        let out = format_reply(Ok(ok), false);
        assert!(out.contains("caches are safe"));
        assert!(out.contains("🪙"));
        assert!(out.contains("$0.2500"));
        let err = format_reply(Err("boom".into()), false);
        assert!(err.starts_with("⚠️"));
    }
}
