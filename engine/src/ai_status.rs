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

/// The current AI backend's readiness: `(lamp, short label)`. Does a live Ollama
/// probe only when Ollama is the selected provider; otherwise no network.
pub fn status() -> (Lamp, String) {
    // Ollama is selected: we can cheaply ask its /api/tags, so report the real
    // reachability (green when it answers, red when it doesn't).
    if ai::use_ollama() {
        return match crate::ollama::list_models() {
            Ok(models) => (
                Lamp::Green,
                format!(
                    "Ollama · {} ({} {})",
                    t("connected", "接続OK"),
                    models.len(),
                    t("models", "モデル"),
                ),
            ),
            Err(_) => (
                Lamp::Red,
                format!("Ollama · {}", t("not reachable", "接続できません")),
            ),
        };
    }
    // Claude Code: the `claude` binary is installed and logged in — ready to run.
    if ai::use_claude_cli() {
        return (
            Lamp::Green,
            t("Claude Code ready", "Claude Code 利用可").to_string(),
        );
    }
    // A BYOK HTTP key is present. We haven't called the API (it would cost), so we
    // say "configured", not "connected".
    if ai::resolve_provider().is_ok() {
        return (
            Lamp::Green,
            t("API key configured", "APIキー設定済み").to_string(),
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
         <span id='ds-ai-lamp'>🟡 {checking}</span></div>\
         <script>(function(){{var e=document.getElementById('ds-ai-lamp');if(!e)return;\
         fetch('/ai-status').then(function(r){{return r.json();}}).then(function(j){{\
         var d={{green:'🟢',yellow:'🟡',red:'🔴'}}[j.lamp]||'🟡';\
         e.textContent=d+' '+(j.label||'');}}).catch(function(){{}});}})();</script>"
    )
}

#[cfg(test)]
mod tests {
    use super::json;

    #[test]
    fn json_is_well_formed() {
        // Whatever the environment, the payload parses and carries a known lamp.
        let v: serde_json::Value = serde_json::from_str(&json()).expect("valid json");
        let lamp = v["lamp"].as_str().unwrap();
        assert!(matches!(lamp, "green" | "yellow" | "red"), "lamp: {lamp}");
        assert!(v["label"].is_string(), "label present");
    }
}
