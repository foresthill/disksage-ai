//! AI mode (BYOK) — request building and response parsing, ported from the bash
//! `build_ai_request` / `parse_ai_response`. Everything sent is METADATA ONLY:
//! masked paths, sizes and heuristic descriptions — never file contents. Paths
//! are run through `mask` first (this module never sends a raw path).
//!
//! This file is pure logic (no network); the actual HTTP send is a later step.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::mask;
use crate::scan::Finding;

/// One per-finding judgment from the model.
#[derive(Debug, Deserialize)]
pub struct Judgment {
    pub index: usize,
    pub recommendation: String, // safe_to_delete | archive_then_delete | review_first | keep
    pub confidence: String,     // high | medium | low
    #[serde(default)]
    pub reasoning: String,
}

fn output_schema() -> Value {
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

/// Build the Claude Messages API request body (JSON string). Findings are masked
/// here, so the returned body never contains a raw user path.
pub fn build_request(findings: &[Finding], model: &str, lang_ja: bool, host_os: &str) -> String {
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

    let user = format!(
        "Host OS: {host_os}\nFindings (metadata only — no file contents):\n{}\n\n\
         Return a judgment for every finding, referenced by its index.",
        serde_json::to_string_pretty(&items).unwrap_or_default()
    );

    json!({
        "model": model,
        "max_tokens": 4096,
        "system": system,
        "output_config": {"format": {"type": "json_schema", "schema": output_schema()}},
        "messages": [{"role": "user", "content": user}],
    })
    .to_string()
}

/// Parse a Claude API response into judgments, surfacing API/refusal errors.
pub fn parse_response(raw: &str) -> Result<Vec<Judgment>, String> {
    let data: Value = serde_json::from_str(raw).map_err(|e| format!("response was not JSON: {e}"))?;

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

    let mut out = Vec::with_capacity(arr.len());
    for j in arr {
        out.push(
            serde_json::from_value::<Judgment>(j.clone())
                .map_err(|e| format!("bad judgment entry: {e}"))?,
        );
    }
    Ok(out)
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
        let f = vec![finding("node_modules_aggregate", "~/Development/SecretProject/node_modules")];
        let body = build_request(&f, "claude-x", true, "macos");
        assert!(body.contains("<dir1>"), "user dir must be anonymized");
        assert!(!body.contains("SecretProject"), "raw user name must NOT be sent");
        assert!(body.contains("metadata only"));
        assert!(body.contains("Japanese"), "lang_ja adds the reasoning-language line");
        assert!(body.contains("\"model\":\"claude-x\""));
    }

    #[test]
    fn parses_a_normal_response() {
        let raw = r#"{"content":[{"type":"text","text":"{\"judgments\":[{\"index\":0,\"recommendation\":\"safe_to_delete\",\"confidence\":\"high\",\"reasoning\":\"regenerable cache\"}]}"}],"usage":{"input_tokens":10}}"#;
        let js = parse_response(raw).expect("should parse");
        assert_eq!(js.len(), 1);
        assert_eq!(js[0].index, 0);
        assert_eq!(js[0].recommendation, "safe_to_delete");
        assert_eq!(js[0].confidence, "high");
    }

    #[test]
    fn surfaces_api_error_and_refusal() {
        let err = r#"{"type":"error","error":{"type":"authentication_error","message":"bad key"}}"#;
        assert!(parse_response(err).unwrap_err().contains("authentication_error"));
        let refusal = r#"{"stop_reason":"refusal","content":[]}"#;
        assert!(parse_response(refusal).unwrap_err().contains("declined"));
    }
}
