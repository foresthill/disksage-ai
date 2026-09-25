//! AI judgment via the user's installed **Claude Code** (`claude -p`), so no
//! separate API key is needed — it runs their logged-in `claude` and reads the
//! structured JSON result. Chosen automatically when no BYOK key is set, or
//! explicitly with `DISKSAGE_AI_PROVIDER=claude-cli`.
//!
//! Privacy is unchanged: the prompt is built by `ai::build_prompt`, which masks
//! all paths — only metadata leaves, never file contents. The trade-off vs. the
//! HTTP API is cost: each `claude -p` call carries Claude Code's own large
//! context, so it is heavier per call (we surface the reported cost in the UI).

use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::Value;

use crate::ai::{self, Analysis, Judgment, Usage};
use crate::audit;
use crate::scan::Finding;
use crate::util::home_dir;

/// Locate the `claude` binary without spawning a process (this is called on every
/// page render). `DISKSAGE_CLAUDE_BIN` overrides; otherwise check the usual spots
/// — a Finder-launched .app has a minimal PATH, so absolute paths matter.
pub fn available() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("DISKSAGE_CLAUDE_BIN").map(PathBuf::from) {
        if p.is_file() {
            return Some(p);
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(home) = home_dir() {
        candidates.push(home.join(".local/bin/claude"));
        candidates.push(home.join(".claude/local/claude"));
        candidates.push(home.join(".bun/bin/claude"));
    }
    for p in [
        "/opt/homebrew/bin/claude",
        "/usr/local/bin/claude",
        "/usr/bin/claude",
    ] {
        candidates.push(PathBuf::from(p));
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// Run `claude -p … --output-format json --json-schema …` and parse the result.
pub fn run(findings: &[Finding], lang_ja: bool, audit_log: bool) -> Result<Analysis, String> {
    let bin = available().ok_or("claude CLI not found (install Claude Code, or set an API key)")?;
    let prompt = ai::build_prompt(findings, lang_ja, std::env::consts::OS);
    let schema = ai::output_schema().to_string();

    // Audit: record the exact prompt + masking table before the call.
    let log = if audit_log {
        match audit::start() {
            Ok(a) => {
                eprintln!("DiskSage: AI audit log → {}", a.dir().display());
                let _ = a.write_request(&prompt);
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

    let mut cmd = Command::new(&bin);
    cmd.arg("-p")
        .arg(&prompt)
        .arg("--output-format")
        .arg("json")
        .arg("--json-schema")
        .arg(&schema)
        .stdin(Stdio::null()); // -p otherwise waits on stdin
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
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("claude produced no output: {}", err.trim()));
    }
    parse(&raw)
}

/// Parse the `claude -p --output-format json` envelope into an `Analysis`.
pub fn parse(raw: &str) -> Result<Analysis, String> {
    let data: Value =
        serde_json::from_str(raw).map_err(|e| format!("claude output was not JSON: {e}"))?;

    if data.get("is_error").and_then(Value::as_bool) == Some(true)
        || data.get("subtype").and_then(Value::as_str) == Some("error")
    {
        let msg = data
            .get("result")
            .and_then(Value::as_str)
            .or_else(|| data.get("error").and_then(Value::as_str))
            .unwrap_or("unknown error");
        return Err(format!("claude error: {msg}"));
    }

    let arr = data
        .get("structured_output")
        .and_then(|s| s.get("judgments"))
        .and_then(Value::as_array)
        .ok_or("claude output had no structured judgments")?;
    let mut judgments = Vec::with_capacity(arr.len());
    for j in arr {
        judgments.push(
            serde_json::from_value::<Judgment>(j.clone())
                .map_err(|e| format!("bad judgment entry: {e}"))?,
        );
    }

    let usage = data
        .get("usage")
        .and_then(|u| serde_json::from_value::<Usage>(u.clone()).ok());
    let cost_usd = data.get("total_cost_usd").and_then(Value::as_f64);

    Ok(Analysis {
        judgments,
        usage,
        cost_usd,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_cli_envelope() {
        // Trimmed from a real `claude -p --output-format json --json-schema` run.
        let raw = r#"{"type":"result","subtype":"success","is_error":false,
          "result":"prose summary",
          "total_cost_usd":1.0156785,
          "usage":{"input_tokens":8947,"output_tokens":647},
          "structured_output":{"judgments":[
            {"index":0,"recommendation":"review_first","confidence":"medium","reasoning":"shared cache container"}
          ]}}"#;
        let a = parse(raw).expect("should parse");
        assert_eq!(a.judgments.len(), 1);
        assert_eq!(a.judgments[0].recommendation, "review_first");
        let u = a.usage.expect("usage");
        assert_eq!(u.input_tokens, 8947);
        assert_eq!(u.output_tokens, 647);
        assert!((a.cost_usd.unwrap() - 1.0156785).abs() < 1e-9);
    }

    #[test]
    fn surfaces_cli_error() {
        let raw = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"boom"}"#;
        assert!(parse(raw).unwrap_err().contains("boom"));
    }
}
