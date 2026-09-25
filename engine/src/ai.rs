//! AI mode (BYOK) — request building and response parsing, ported from the bash
//! `build_ai_request` / `parse_ai_response`. Everything sent is METADATA ONLY:
//! masked paths, sizes and heuristic descriptions — never file contents. Paths
//! are run through `mask` first (this module never sends a raw path).
//!
//! This file is pure logic (no network); the actual HTTP send is a later step.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::audit;
use crate::mask;
use crate::scan::Finding;

const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const MODEL_ANTHROPIC: &str = "claude-opus-4-8";
const MODEL_OPENROUTER: &str = "anthropic/claude-opus-4.8";

/// How to authenticate: Anthropic direct (x-api-key) or OpenRouter (Bearer).
pub enum Auth {
    XApiKey,
    Bearer,
}

/// A resolved BYOK provider. Never logged — carries the user's key.
pub struct Provider {
    pub url: String,
    pub key: String,
    pub auth: Auth,
    pub version: Option<String>,
    pub model: String,
}

/// Pure provider resolution (env read separately, so this is testable).
/// Precedence: explicit provider > Anthropic key > OpenRouter key.
fn resolve_from(
    pref: &str,
    anthropic: Option<String>,
    openrouter: Option<String>,
    base_url: Option<String>,
    model_override: Option<String>,
) -> Result<Provider, String> {
    let want_anthropic = pref == "anthropic" || (pref.is_empty() && anthropic.is_some());
    if want_anthropic {
        let key = anthropic.ok_or("ANTHROPIC_API_KEY is not set")?;
        return Ok(Provider {
            url: base_url.unwrap_or_else(|| ANTHROPIC_URL.into()),
            key,
            auth: Auth::XApiKey,
            version: Some(ANTHROPIC_VERSION.into()),
            model: model_override.unwrap_or_else(|| MODEL_ANTHROPIC.into()),
        });
    }
    let want_openrouter = pref == "openrouter" || (pref.is_empty() && openrouter.is_some());
    if want_openrouter {
        let key = openrouter.ok_or("OPENROUTER_API_KEY is not set")?;
        return Ok(Provider {
            url: base_url.unwrap_or_else(|| OPENROUTER_URL.into()),
            key,
            auth: Auth::Bearer,
            version: None,
            model: model_override.unwrap_or_else(|| MODEL_OPENROUTER.into()),
        });
    }
    Err("no API key found — set ANTHROPIC_API_KEY or OPENROUTER_API_KEY".into())
}

/// Whether to route the analysis through the `claude` CLI instead of a BYOK HTTP
/// call. Precedence: explicit DISKSAGE_AI_PROVIDER=claude-cli → yes; any other
/// explicit provider → no; otherwise auto (no key set AND `claude` is installed).
fn use_claude_cli() -> bool {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    match env("DISKSAGE_AI_PROVIDER").as_deref() {
        Some("claude-cli") => true,
        Some(_) => false,
        None => {
            env("ANTHROPIC_API_KEY").is_none()
                && env("OPENROUTER_API_KEY").is_none()
                && crate::claude_cli::available().is_some()
        }
    }
}

/// True when an AI backend is usable: a BYOK HTTP key, or the `claude` CLI.
pub fn available() -> bool {
    use_claude_cli() || resolve_provider().is_ok()
}

/// Resolve the provider from the environment (BYOK).
pub fn resolve_provider() -> Result<Provider, String> {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    resolve_from(
        &env("DISKSAGE_AI_PROVIDER").unwrap_or_default(),
        env("ANTHROPIC_API_KEY"),
        env("OPENROUTER_API_KEY"),
        env("DISKSAGE_AI_BASE_URL"),
        env("DISKSAGE_MODEL"),
    )
}

/// Mask → build → POST (BYOK) → parse. The only place a request leaves the
/// machine; the body is metadata-only and paths are already masked in build_request.
/// Audit logging is enabled by `--ai-log` / `DISKSAGE_AI_LOG=1`.
pub fn analyze(findings: &[Finding], lang_ja: bool) -> Result<Analysis, String> {
    analyze_with_audit(findings, lang_ja, audit::enabled(false))
}

/// As `analyze`, but with the audit-log decision passed in explicitly (so the
/// CLI `--ai-log` flag works even without the env var). When on, the exact
/// request body, the raw response and the real→masked path table are written to
/// `$DISKSAGE_HOME/ai-logs/<stamp>/` before/after the send.
pub fn analyze_with_audit(
    findings: &[Finding],
    lang_ja: bool,
    audit_log: bool,
) -> Result<Analysis, String> {
    // Prefer your installed, logged-in Claude Code (`claude -p`) when chosen or
    // when no BYOK key is set — no separate API key needed.
    if use_claude_cli() {
        return crate::claude_cli::run(findings, lang_ja, audit_log);
    }
    let p = resolve_provider()?;
    let body = build_request(findings, &p.model, lang_ja, std::env::consts::OS);

    // Record the request + masking table before the send, so an audit exists even
    // if the network call fails. Logging failures must never block the analysis.
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

    let mut req = ureq::post(&p.url).set("content-type", "application/json");
    match p.auth {
        Auth::XApiKey => req = req.set("x-api-key", &p.key),
        Auth::Bearer => req = req.set("authorization", &format!("Bearer {}", p.key)),
    }
    if let Some(v) = &p.version {
        req = req.set("anthropic-version", v);
    }
    // Read the body on both success and HTTP-error (API errors are JSON that
    // parse_response turns into a clear message).
    let text = match req.send_string(&body) {
        Ok(r) => r.into_string().map_err(|e| e.to_string())?,
        Err(ureq::Error::Status(_, r)) => r.into_string().map_err(|e| e.to_string())?,
        Err(e) => return Err(format!("request failed: {e}")),
    };
    if let Some(a) = &log {
        let _ = a.write_response(&text);
    }
    parse_response(&text)
}

/// One per-finding judgment from the model.
#[derive(Debug, Deserialize)]
pub struct Judgment {
    pub index: usize,
    pub recommendation: String, // safe_to_delete | archive_then_delete | review_first | keep
    pub confidence: String,     // high | medium | low
    #[serde(default)]
    pub reasoning: String,
}

/// Token usage reported by the API (metadata about the call itself).
#[derive(Debug, Default, Clone, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

/// The result of an AI analysis: the per-finding judgments plus the token usage,
/// so callers can show "how much did this cost" without re-reading the raw log.
#[derive(Debug)]
pub struct Analysis {
    pub judgments: Vec<Judgment>,
    pub usage: Option<Usage>,
    /// Cost in USD when the backend reports it (the `claude` CLI does; the raw
    /// HTTP API does not, and we don't guess per-model pricing).
    pub cost_usd: Option<f64>,
}

pub fn output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "judgments": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "index": {"type": "integer"},
                        "recommendation": {"type": "string",
                            "enum": ["safe_to_delete","archive_then_delete","review_first","keep"]},
                        "confidence": {"type": "string", "enum": ["high","medium","low"]},
                        "reasoning": {"type": "string"}
                    },
                    "required": ["index","recommendation","confidence","reasoning"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["judgments"],
        "additionalProperties": false
    })
}

/// The system instructions, shared by the HTTP body and the CLI prompt.
fn system_text(lang_ja: bool) -> String {
    let mut system = String::from(
        "You are a disk-cleanup advisor for DiskSage, a tool that NEVER deletes anything \
         automatically — it only suggests. You receive disk-space findings as METADATA ONLY: \
         file paths, sizes, and heuristic descriptions. You never see file contents. For each \
         finding, judge how safe it is to reclaim the space and explain why, briefly, for a \
         technical user. Be conservative: when a path may hold irreplaceable user data (device \
         backups, documents, photos, original media), prefer 'review_first' or \
         'archive_then_delete'. Caches, build artifacts, swap files, and trivially regenerable \
         data can be 'safe_to_delete'. Never recommend deleting something the user cannot \
         regenerate without archiving it first. Return one judgment per finding, keyed by its index.",
    );
    if lang_ja {
        system.push_str(
            " Write every \"reasoning\" value in Japanese. Keep the JSON keys and the \
             recommendation/confidence enum values in English.",
        );
    }
    system
}

/// The user turn: masked findings as metadata. Paths are masked here, so this
/// never contains a raw user path (whether sent over HTTP or to the CLI).
fn user_text(findings: &[Finding], host_os: &str) -> String {
    let mut aliases = mask::Aliases::new();
    let items: Vec<Value> = findings
        .iter()
        .enumerate()
        .map(|(i, f)| {
            json!({
                "index": i,
                "pattern_id": f.id,
                "path": mask::mask_path(&f.path, &mut aliases),
                "size_bytes": f.size,
                "heuristic_severity": f.severity,
                "description": f.description,
            })
        })
        .collect();
    format!(
        "Host OS: {host_os}\nFindings (metadata only — no file contents):\n{}\n\n\
         Return a judgment for every finding, referenced by its index.",
        serde_json::to_string_pretty(&items).unwrap_or_default()
    )
}

/// Build the Claude Messages API request body (JSON string). Findings are masked
/// in `user_text`, so the returned body never contains a raw user path.
pub fn build_request(findings: &[Finding], model: &str, lang_ja: bool, host_os: &str) -> String {
    json!({
        "model": model,
        "max_tokens": 4096,
        "system": system_text(lang_ja),
        "output_config": {"format": {"type": "json_schema", "schema": output_schema()}},
        "messages": [{"role": "user", "content": user_text(findings, host_os)}],
    })
    .to_string()
}

/// Build a single prompt string for the `claude -p` CLI path (system + findings).
/// Same masked metadata as `build_request`; the schema is passed via --json-schema.
pub fn build_prompt(findings: &[Finding], lang_ja: bool, host_os: &str) -> String {
    format!(
        "{}\n\n{}",
        system_text(lang_ja),
        user_text(findings, host_os)
    )
}

/// Parse a Claude API response into judgments + usage, surfacing API/refusal errors.
pub fn parse_response(raw: &str) -> Result<Analysis, String> {
    let data: Value =
        serde_json::from_str(raw).map_err(|e| format!("response was not JSON: {e}"))?;

    if data.get("type").and_then(Value::as_str) == Some("error") {
        let err = data.get("error").cloned().unwrap_or(Value::Null);
        return Err(format!(
            "API error: {}: {}",
            err.get("type").and_then(Value::as_str).unwrap_or("?"),
            err.get("message").and_then(Value::as_str).unwrap_or("?"),
        ));
    }
    if data.get("stop_reason").and_then(Value::as_str) == Some("refusal") {
        return Err("the model declined to analyze these findings".into());
    }

    // Token usage is on the top-level response object (not in the content block).
    let usage = data
        .get("usage")
        .and_then(|u| serde_json::from_value::<Usage>(u.clone()).ok());

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
        .ok_or("response contained no text block")?;

    let payload: Value =
        serde_json::from_str(text).map_err(|e| format!("structured output was not JSON: {e}"))?;
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
    Ok(Analysis {
        judgments,
        usage,
        cost_usd: None, // the HTTP API doesn't report a dollar cost
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
            size: 1024,
            severity: "low",
            description: "example".to_string(),
            action: "example".to_string(),
        }
    }

    #[test]
    fn request_masks_paths_and_never_leaks_real_names() {
        let f = vec![finding(
            "node_modules_aggregate",
            "~/Development/SecretProject/node_modules",
        )];
        let body = build_request(&f, "claude-x", true, "macos");
        assert!(body.contains("<dir1>"), "user dir must be anonymized");
        assert!(
            !body.contains("SecretProject"),
            "raw user name must NOT be sent"
        );
        assert!(body.contains("metadata only"));
        assert!(
            body.contains("Japanese"),
            "lang_ja adds the reasoning-language line"
        );
        assert!(body.contains("\"model\":\"claude-x\""));
    }

    #[test]
    fn parses_a_normal_response() {
        let raw = r#"{"content":[{"type":"text","text":"{\"judgments\":[{\"index\":0,\"recommendation\":\"safe_to_delete\",\"confidence\":\"high\",\"reasoning\":\"regenerable cache\"}]}"}],"usage":{"input_tokens":10,"output_tokens":5}}"#;
        let a = parse_response(raw).expect("should parse");
        assert_eq!(a.judgments.len(), 1);
        assert_eq!(a.judgments[0].index, 0);
        assert_eq!(a.judgments[0].recommendation, "safe_to_delete");
        assert_eq!(a.judgments[0].confidence, "high");
        let u = a.usage.expect("usage parsed");
        assert_eq!(u.input_tokens, 10);
        assert_eq!(u.output_tokens, 5);
    }

    #[test]
    fn provider_resolution_precedence_and_headers() {
        let s = |x: &str| Some(x.to_string());
        // Anthropic wins when both keys present and no explicit pref.
        let p = resolve_from("", s("ak"), s("ok"), None, None).unwrap();
        assert!(matches!(p.auth, Auth::XApiKey));
        assert_eq!(p.url, ANTHROPIC_URL);
        assert_eq!(p.version.as_deref(), Some(ANTHROPIC_VERSION));
        assert_eq!(p.model, MODEL_ANTHROPIC);
        // OpenRouter when only its key is set.
        let p = resolve_from("", None, s("ok"), None, None).unwrap();
        assert!(matches!(p.auth, Auth::Bearer));
        assert_eq!(p.url, OPENROUTER_URL);
        assert!(p.version.is_none());
        // Explicit pref overrides key presence; base_url/model overrides honored.
        let p = resolve_from("openrouter", s("ak"), s("ok"), s("http://x"), s("m")).unwrap();
        assert_eq!(p.url, "http://x");
        assert_eq!(p.model, "m");
        // No keys → error.
        assert!(resolve_from("", None, None, None, None).is_err());
    }

    #[test]
    fn surfaces_api_error_and_refusal() {
        let err = r#"{"type":"error","error":{"type":"authentication_error","message":"bad key"}}"#;
        assert!(parse_response(err)
            .unwrap_err()
            .contains("authentication_error"));
        let refusal = r#"{"stop_reason":"refusal","content":[]}"#;
        assert!(parse_response(refusal).unwrap_err().contains("declined"));
    }
}
