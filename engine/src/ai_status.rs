//! A truthful "is the AI reachable?" lamp (🟢 / 🟡 / 🔴) for the UI.
//!
//! Served on demand via `GET /ai-status` and filled in by a tiny client script,
//! so a slow Ollama probe never blocks the page render (the Scan page stays
//! responsive, consistent with how AI reachability is otherwise only checked at
//! send time).
//!
//! Honesty rule: only Ollama is actually probed here (a cheap localhost call we
//! can afford). For a BYOK HTTP key or Claude Code we report that the backend is
//! *configured* — not that we reached it. Verifying those needs a paid/slow call,
//! and we never claim connectivity we didn't check.

use crate::ai;
use crate::lang::t;

pub enum Lamp {
    Green,
    Yellow,
    Red,
}

impl Lamp {
    /// Machine key for the JSON payload / client script.
    pub fn key(&self) -> &'static str {
        match self {
            Lamp::Green => "green",
            Lamp::Yellow => "yellow",
            Lamp::Red => "red",
        }
    }
}

/// `host:port` from a base URL, dropping the scheme and any path so the endpoint
/// reads as a compact chip. `http://localhost:11434` → `localhost:11434`.
fn host_port(url: &str) -> String {
    let no_scheme = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);
    no_scheme.split('/').next().unwrap_or(no_scheme).to_string()
}

/// A friendly BYOK provider name inferred from its endpoint URL.
fn byok_provider_name(url: &str) -> &'static str {
    if url.contains("openrouter") {
        "OpenRouter"
    } else {
        "Anthropic"
    }
}

/// The current AI backend's readiness: `(lamp, label)`. The label is a ` · `
/// separated chip — provider, model, state, and (for Ollama) the endpoint — so the
/// user can see *which* model and *where* at a glance. Does a live Ollama probe
/// only when Ollama is the selected provider; otherwise no network.
pub fn status() -> (Lamp, String) {
    // Ollama is selected: we can cheaply ask its /api/tags, so report the real
    // reachability (green when it answers, red when it doesn't). Show the chosen
    // model and the endpoint host:port either way.
    if ai::use_ollama() {
        let host = host_port(&crate::ollama::base_url());
        let model = crate::ollama::model();
        return match crate::ollama::list_models() {
            Ok(models) => {
                // Prefer the picked model; fall back to the installed count when
                // none is selected yet (still useful: "it's up, N models there").
                let which = if model.is_empty() {
                    format!("{} {}", models.len(), t("models", "モデル"))
                } else {
                    model
                };
                (
                    Lamp::Green,
                    format!("Ollama · {which} · {} · {host}", t("connected", "接続OK")),
                )
            }
            Err(_) => {
                let which = if model.is_empty() {
                    t("no model", "モデル未選択").to_string()
                } else {
                    model
                };
                (
                    Lamp::Red,
                    format!(
                        "Ollama · {which} · {} · {host}",
                        t("not reachable", "接続できません")
                    ),
                )
            }
        };
    }
    // Claude Code: the `claude` binary is installed and logged in — ready to run.
    // It uses its own default model unless DISKSAGE_MODEL overrides it; show that
    // only when set (we don't guess the default).
    if ai::use_claude_cli() {
        let label = match std::env::var("DISKSAGE_MODEL")
            .ok()
            .filter(|v| !v.is_empty())
        {
            Some(model) => format!("Claude Code · {model} · {}", t("ready", "利用可")),
            None => t("Claude Code ready", "Claude Code 利用可").to_string(),
        };
        return (Lamp::Green, label);
    }
    // A BYOK HTTP key is present. We haven't called the API (it would cost), so we
    // say "configured", not "connected" — and show the provider + model we'd use.
    if let Ok(p) = ai::resolve_provider() {
        let name = byok_provider_name(&p.url);
        return (
            Lamp::Green,
            format!(
                "{name} · {} · {}",
                p.model,
                t("API key configured", "APIキー設定済み")
            ),
        );
    }
    // Nothing set up.
    (
        Lamp::Yellow,
        t("No AI backend configured", "AI未設定").to_string(),
    )
}

/// JSON payload for `GET /ai-status`: `{"lamp":"green|yellow|red","label":"…"}`.
pub fn json() -> String {
    let (lamp, label) = status();
    let label = serde_json::to_string(&label).unwrap_or_else(|_| "\"\"".to_string());
    format!("{{\"lamp\":\"{}\",\"label\":{}}}", lamp.key(), label)
}

/// The lamp placeholder + the one-shot script that fills it from `/ai-status`.
/// Rendered in the Scan page's AI area; starts as 🟡 "checking…" and updates in
/// place without reloading or scanning.
pub fn lamp_html() -> String {
    let checking = t("checking…", "確認中…");
    format!(
        "<div style='font-size:13px;color:#57606a;margin-bottom:6px'>\
         <span id='ds-ai-lamp' style='display:inline-block;background:#f6f8fa;\
         border:1px solid #d0d7de;border-radius:12px;padding:3px 11px;font-size:12px;\
         color:#1f2328'>🟡 {checking}</span></div>\
         <script>(function(){{var e=document.getElementById('ds-ai-lamp');if(!e)return;\
         fetch('/ai-status').then(function(r){{return r.json();}}).then(function(j){{\
         var d={{green:'🟢',yellow:'🟡',red:'🔴'}}[j.lamp]||'🟡';\
         e.textContent=d+' '+(j.label||'');}}).catch(function(){{}});}})();</script>"
    )
}

#[cfg(test)]
mod tests {
    use super::{byok_provider_name, host_port, json};

    #[test]
    fn json_is_well_formed() {
        // Whatever the environment, the payload parses and carries a known lamp.
        let v: serde_json::Value = serde_json::from_str(&json()).expect("valid json");
        let lamp = v["lamp"].as_str().unwrap();
        assert!(matches!(lamp, "green" | "yellow" | "red"), "lamp: {lamp}");
        assert!(v["label"].is_string(), "label present");
    }

    #[test]
    fn host_port_drops_scheme_and_path() {
        assert_eq!(host_port("http://localhost:11434"), "localhost:11434");
        assert_eq!(host_port("https://1.2.3.4:443/api"), "1.2.3.4:443");
        assert_eq!(host_port("localhost:11435"), "localhost:11435");
    }

    #[test]
    fn byok_provider_name_from_url() {
        assert_eq!(
            byok_provider_name("https://openrouter.ai/api/v1/messages"),
            "OpenRouter"
        );
        assert_eq!(
            byok_provider_name("https://api.anthropic.com/v1/messages"),
            "Anthropic"
        );
    }
}
