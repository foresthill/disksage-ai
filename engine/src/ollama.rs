//! Ollama provider — run the AI judgment against a local (or port-forwarded)
//! Ollama server. No API key: Ollama runs on the user's own machine.
//!
//! Privacy is unchanged from the HTTP/CLI paths: findings go through the same
//! masking (`ai::user_text` → `ai_preview::rows`), so only METADATA is sent —
//! masked paths, sizes and heuristic descriptions, never file contents.
//!
//! Uses Ollama's native API (confirmed against https://docs.ollama.com/api):
//!   - `POST /api/chat`  with `format` = our JSON schema → structured output
//!   - `GET  /api/tags`  → the installed model names (Settings dropdown + test)
//!
//! The base URL is configurable (`ollama_url` in config, or `DISKSAGE_OLLAMA_URL`)
//! because port-forwarding can move it off the default `localhost:11434`.

use std::time::Duration;

use serde_json::{json, Value};

use crate::ai::{self, Analysis, Judgment, Usage};
use crate::audit;
use crate::scan::Finding;

/// Ollama's default local endpoint.
pub const DEFAULT_URL: &str = "http://localhost:11434";

/// Effective base URL: env override → config `ollama_url` → default. Any trailing
/// slash is trimmed so `{base}/api/...` never doubles up.
pub fn base_url() -> String {
    let raw = std::env::var("DISKSAGE_OLLAMA_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| crate::settings::read_key("ollama_url"));
    let raw = raw.trim();
    let url = if raw.is_empty() { DEFAULT_URL } else { raw };
    url.trim_end_matches('/').to_string()
}

/// The chosen model: env override → config `ollama_model` ("" when unset).
pub fn model() -> String {
    std::env::var("DISKSAGE_OLLAMA_MODEL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| crate::settings::read_key("ollama_model"))
}

/// Turn a ureq error into a short, user-facing message (no stack noise).
fn friendly_err(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, _) => format!("HTTP {code}"),
        ureq::Error::Transport(t) => format!("cannot reach Ollama — {t}"),
    }
}

/// `GET /api/tags` → installed model names. Short timeout so the Settings page
/// stays responsive when Ollama isn't running (a localhost refusal is instant;
/// a wrong remote host is capped at the timeout).
pub fn list_models() -> Result<Vec<String>, String> {
    let url = format!("{}/api/tags", base_url());
    let text = ureq::get(&url)
        .timeout(Duration::from_secs(3))
        .call()
        .map_err(friendly_err)?
        .into_string()
        .map_err(|e| e.to_string())?;
    let data: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(data
        .get("models")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("name").and_then(Value::as_str).map(String::from))
                .collect()
        })
        .unwrap_or_default())
}

/// Mask → build the native `/api/chat` body → POST → parse. Metadata only; the
/// body is built from masked findings in `build_request`, never a raw user path.
/// Audit logging (`--ai-log` / `DISKSAGE_AI_LOG=1`) writes the request, masking
/// table and raw response, exactly like the HTTP/CLI paths.
pub fn run(findings: &[Finding], lang_ja: bool, audit_log: bool) -> Result<Analysis, String> {
    let model = model();
    if model.is_empty() {
        return Err("no Ollama model selected — pick one in Settings".into());
    }
    let body = build_request(findings, &model, lang_ja, std::env::consts::OS);

    // Record request + masking before the send, so an audit exists even if the
    // call fails. Logging failures never block the analysis.
    let log = if audit_log {
        match audit::start() {
            Ok(a) => {
                eprintln!("DiskSage: AI audit log → {}", a.dir().display());
                let _ = a.write_request(&body);
                let _ = a.write_masking(findings);
                Some(a)
            }
            Err(e) => {
                eprintln!("DiskSage: could not start AI audit log: {e}");
                None
            }
        }
    } else {
        None
    };

    let url = format!("{}/api/chat", base_url());
    // No short timeout here: a local model can take a while to generate. Read the
    // body on HTTP-error too (Ollama returns a JSON {"error": ...}).
    let text = match ureq::post(&url)
        .set("content-type", "application/json")
        .send_string(&body)
    {
        Ok(r) => r.into_string().map_err(|e| e.to_string())?,
        Err(ureq::Error::Status(_, r)) => r.into_string().map_err(|e| e.to_string())?,
        Err(e) => return Err(friendly_err(e)),
    };
    if let Some(a) = &log {
        let _ = a.write_response(&text);
    }
    parse_response(&text)
}

/// The native `/api/chat` body: system + user messages, structured `format`, no
/// streaming. Reuses `ai::system_text` / `ai::user_text` so the prompt — and the
/// masking — is identical to the Anthropic/Claude-CLI paths.
pub fn build_request(findings: &[Finding], model: &str, lang_ja: bool, host_os: &str) -> String {
    json!({
        "model": model,
        "stream": false,
        "format": ai::output_schema(),
        "messages": [
            {"role": "system", "content": ai::system_text(lang_ja)},
            {"role": "user", "content": ai::user_text(findings, host_os)},
        ],
    })
    .to_string()
}

/// Parse an `/api/chat` response: `message.content` is the JSON our schema asked
/// for. Token usage comes from `prompt_eval_count` / `eval_count`. No dollar cost
/// (local inference is free — we never guess a price).
pub fn parse_response(raw: &str) -> Result<Analysis, String> {
    let data: Value =
        serde_json::from_str(raw).map_err(|e| format!("response was not JSON: {e}"))?;
    if let Some(err) = data.get("error").and_then(Value::as_str) {
        return Err(format!("Ollama error: {err}"));
    }
    let content = data
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .ok_or("response had no message.content")?;
    let payload: Value = serde_json::from_str(content)
        .map_err(|e| format!("structured output was not JSON: {e}"))?;
    let arr = payload
        .get("judgments")
        .and_then(Value::as_array)
        .ok_or("response had no judgments array")?;
    let mut judgments = Vec::with_capacity(arr.len());
    for j in arr {
        judgments.push(
            serde_json::from_value::<Judgment>(j.clone())
                .map_err(|e| format!("bad judgment entry: {e}"))?,
        );
    }
    let usage = Usage {
        input_tokens: data
            .get("prompt_eval_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        output_tokens: data.get("eval_count").and_then(Value::as_u64).unwrap_or(0),
    };
    Ok(Analysis {
        judgments,
        usage: Some(usage),
        cost_usd: None,
    })
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
    fn build_request_masks_paths_and_sets_format() {
        let f = vec![finding(
            "node_modules_aggregate",
            "~/Development/SecretProj/node_modules",
        )];
        let body = build_request(&f, "llama3.2", false, "macos");
        assert!(body.contains("\"model\":\"llama3.2\""));
        assert!(body.contains("\"stream\":false"));
        assert!(body.contains("\"format\""), "structured output schema sent");
        assert!(body.contains("<dir1>"), "user dir is anonymized");
        assert!(
            !body.contains("SecretProj"),
            "raw user name must NOT be sent"
        );
    }

    #[test]
    fn parses_a_normal_chat_response() {
        // message.content holds the schema JSON; usage from prompt_eval/eval counts.
        let raw = r#"{"model":"llama3.2","message":{"role":"assistant","content":"{\"judgments\":[{\"index\":0,\"recommendation\":\"safe_to_delete\",\"confidence\":\"high\",\"reasoning\":\"regenerable cache\"}]}"},"done":true,"prompt_eval_count":42,"eval_count":12}"#;
        let a = parse_response(raw).expect("should parse");
        assert_eq!(a.judgments.len(), 1);
        assert_eq!(a.judgments[0].recommendation, "safe_to_delete");
        let u = a.usage.expect("usage");
        assert_eq!(u.input_tokens, 42);
        assert_eq!(u.output_tokens, 12);
        assert!(
            a.cost_usd.is_none(),
            "local inference reports no dollar cost"
        );
    }

    #[test]
    fn surfaces_ollama_error() {
        let raw = r#"{"error":"model 'nope' not found"}"#;
        assert!(parse_response(raw).unwrap_err().contains("not found"));
    }

    #[test]
    fn model_list_parses_tags() {
        // Exercising the tags JSON shape (not a live call).
        let data: Value = serde_json::from_str(
            r#"{"models":[{"name":"llama3.2:latest"},{"name":"qwen2.5:7b"}]}"#,
        )
        .unwrap();
        let names: Vec<String> = data["models"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|m| m.get("name").and_then(Value::as_str).map(String::from))
            .collect();
        assert_eq!(names, vec!["llama3.2:latest", "qwen2.5:7b"]);
    }
}
