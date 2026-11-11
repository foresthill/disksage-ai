//! Per-provider transport for the follow-up chat (chat.rs holds the dispatch and
//! the prompt). Each returns a plain-text `ChatReply`. Privacy is unchanged: the
//! system prompt (built in chat.rs from masked metadata) is passed in; only the
//! user's own typed turns are added verbatim.

use serde_json::{json, Value};

use crate::ai::{self, Auth, Usage};
use crate::audit;
use crate::chat::{ChatReply, ChatTurn};
use crate::scan::Finding;

/// Start an audit session (request + masking), returning it so the caller can add
/// the response. Never blocks the chat on a logging failure.
fn start_log(audit_log: bool, request: &str, findings: &[Finding]) -> Option<audit::Audit> {
    if !audit_log {
        return None;
    }
    match audit::start() {
        Ok(a) => {
            eprintln!("DiskSage: AI audit log → {}", a.dir().display());
            let _ = a.write_request(request);
            let _ = a.write_masking(findings);
            Some(a)
        }
        Err(e) => {
            eprintln!("DiskSage: could not start AI audit log: {e}");
            None
        }
    }
}

/// Build the `[{role, content}, …]` messages array: system first, then the turns.
fn messages(system: &str, history: &[ChatTurn]) -> Vec<Value> {
    let mut msgs = vec![json!({"role": "system", "content": system})];
    for turn in history {
        msgs.push(json!({"role": turn.role, "content": turn.content}));
    }
    msgs
}

/// Local Ollama `/api/chat` (multi-turn, free text — no `format`).
pub(crate) fn ollama_chat(
    system: &str,
    history: &[ChatTurn],
    findings: &[Finding],
    audit_log: bool,
) -> Result<ChatReply, String> {
    let model = crate::ollama::model();
    if model.is_empty() {
        return Err("no Ollama model selected — pick one in Settings".into());
    }
    let body = json!({
        "model": model,
        "stream": false,
        "messages": messages(system, history),
    })
    .to_string();
    let log = start_log(audit_log, &body, findings);
    let url = format!("{}/api/chat", crate::ollama::base_url());
    let raw = match ureq::post(&url)
        .set("content-type", "application/json")
        .send_string(&body)
    {
        Ok(r) => r.into_string().map_err(|e| e.to_string())?,
        Err(ureq::Error::Status(_, r)) => r.into_string().map_err(|e| e.to_string())?,
        Err(ureq::Error::Transport(t)) => return Err(format!("cannot reach Ollama — {t}")),
    };
    if let Some(a) = &log {
        let _ = a.write_response(&raw);
    }
    let data: Value = serde_json::from_str(&raw).map_err(|e| format!("not JSON: {e}"))?;
    if let Some(err) = data.get("error").and_then(Value::as_str) {
        return Err(format!("Ollama error: {err}"));
    }
    let text = data
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .ok_or("response had no message.content")?
        .to_string();
    let usage = Usage {
        input_tokens: data
            .get("prompt_eval_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        output_tokens: data.get("eval_count").and_then(Value::as_u64).unwrap_or(0),
    };
    Ok(ChatReply {
        text,
        usage: Some(usage),
        cost_usd: None,
    })
}

/// BYOK HTTP (Anthropic / OpenRouter messages API). The `system` goes in its own
/// field; only user/assistant turns go in `messages`.
pub(crate) fn http_chat(
    system: &str,
    history: &[ChatTurn],
    findings: &[Finding],
    audit_log: bool,
) -> Result<ChatReply, String> {
    let p = ai::resolve_provider()?;
    let turns: Vec<Value> = history
        .iter()
        .map(|turn| json!({"role": turn.role, "content": turn.content}))
        .collect();
    let body = json!({
        "model": p.model,
        "max_tokens": 1024,
        "system": system,
        "messages": turns,
    })
    .to_string();
    let log = start_log(audit_log, &body, findings);
    let mut req = ureq::post(&p.url).set("content-type", "application/json");
    match p.auth {
        Auth::XApiKey => req = req.set("x-api-key", &p.key),
        Auth::Bearer => req = req.set("authorization", &format!("Bearer {}", p.key)),
    }
    if let Some(v) = &p.version {
        req = req.set("anthropic-version", v);
    }
    let raw = match req.send_string(&body) {
        Ok(r) => r.into_string().map_err(|e| e.to_string())?,
        Err(ureq::Error::Status(_, r)) => r.into_string().map_err(|e| e.to_string())?,
        Err(e) => return Err(format!("request failed: {e}")),
    };
    if let Some(a) = &log {
        let _ = a.write_response(&raw);
    }
    parse_http(&raw)
}

/// Parse the Anthropic/OpenRouter messages response: first text block + usage.
fn parse_http(raw: &str) -> Result<ChatReply, String> {
    let data: Value = serde_json::from_str(raw).map_err(|e| format!("not JSON: {e}"))?;
    if data.get("type").and_then(Value::as_str) == Some("error") {
        let err = data.get("error").cloned().unwrap_or(Value::Null);
        return Err(format!(
            "API error: {}: {}",
            err.get("type").and_then(Value::as_str).unwrap_or("?"),
            err.get("message").and_then(Value::as_str).unwrap_or("?"),
        ));
    }
    let text = data
        .get("content")
        .and_then(Value::as_array)
        .and_then(|blocks| {
            blocks
                .iter()
                .find(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .and_then(|b| b.get("text"))
                .and_then(Value::as_str)
        })
        .ok_or("response contained no text block")?
        .to_string();
    let usage = data
        .get("usage")
        .and_then(|u| serde_json::from_value::<Usage>(u.clone()).ok());
    Ok(ChatReply {
        text,
        usage,
        cost_usd: None,
    })
}

/// `claude -p` as a plain-text chat: serialize the conversation into one prompt
/// (no `--json-schema`, so `result` is prose).
pub(crate) fn claude_chat(
    system: &str,
    history: &[ChatTurn],
    findings: &[Finding],
    audit_log: bool,
) -> Result<ChatReply, String> {
    let bin = crate::claude_cli::available().ok_or("claude CLI not found (install Claude Code)")?;
    let mut prompt = String::from(system);
    for turn in history {
        let who = if turn.role == "assistant" {
            "Assistant"
        } else {
            "User"
        };
        prompt.push_str(&format!("\n\n{who}: {}", turn.content));
    }
    prompt.push_str("\n\nAssistant:");
    let log = start_log(audit_log, &prompt, findings);
    let mut cmd = std::process::Command::new(&bin);
    cmd.arg("-p")
        .arg(&prompt)
        .arg("--output-format")
        .arg("json")
        .stdin(std::process::Stdio::null());
    if let Ok(model) = std::env::var("DISKSAGE_MODEL") {
        if !model.is_empty() {
            cmd.arg("--model").arg(model);
        }
    }
    let out = cmd
        .output()
        .map_err(|e| format!("failed to run claude: {e}"))?;
    let raw = String::from_utf8_lossy(&out.stdout).into_owned();
    if let Some(a) = &log {
        let _ = a.write_response(&raw);
    }
    if raw.trim().is_empty() {
        return Err(format!(
            "claude produced no output: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    parse_claude(&raw)
}

/// Parse the `claude -p --output-format json` envelope: `result` is the prose.
fn parse_claude(raw: &str) -> Result<ChatReply, String> {
    let data: Value = serde_json::from_str(raw).map_err(|e| format!("not JSON: {e}"))?;
    if data.get("is_error").and_then(Value::as_bool) == Some(true) {
        let msg = data
            .get("result")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        return Err(format!("claude error: {msg}"));
    }
    let text = data
        .get("result")
        .and_then(Value::as_str)
        .ok_or("claude output had no result text")?
        .to_string();
    let usage = data
        .get("usage")
        .and_then(|u| serde_json::from_value::<Usage>(u.clone()).ok());
    let cost_usd = data.get("total_cost_usd").and_then(Value::as_f64);
    Ok(ChatReply {
        text,
        usage,
        cost_usd,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_puts_system_first_then_turns() {
        let h = vec![
            ChatTurn {
                role: "user".into(),
                content: "hi".into(),
            },
            ChatTurn {
                role: "assistant".into(),
                content: "hello".into(),
            },
        ];
        let m = messages("SYS", &h);
        assert_eq!(m.len(), 3);
        assert_eq!(m[0]["role"], "system");
        assert_eq!(m[1]["role"], "user");
        assert_eq!(m[2]["content"], "hello");
    }

    #[test]
    fn parse_http_reads_text_and_usage() {
        let raw = r#"{"content":[{"type":"text","text":"caches are safe"}],"usage":{"input_tokens":12,"output_tokens":7}}"#;
        let r = parse_http(raw).expect("parse");
        assert_eq!(r.text, "caches are safe");
        assert_eq!(r.usage.unwrap().input_tokens, 12);
    }

    #[test]
    fn parse_claude_reads_result_and_cost() {
        let raw = r#"{"type":"result","is_error":false,"result":"that is a cache","total_cost_usd":0.5,"usage":{"input_tokens":9,"output_tokens":4}}"#;
        let r = parse_claude(raw).expect("parse");
        assert_eq!(r.text, "that is a cache");
        assert!((r.cost_usd.unwrap() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn parse_http_surfaces_api_error() {
        let raw = r#"{"type":"error","error":{"type":"authentication_error","message":"bad key"}}"#;
        assert!(parse_http(raw)
            .unwrap_err()
            .contains("authentication_error"));
    }
}
