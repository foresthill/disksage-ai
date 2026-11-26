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

/// The role of a chip segment, so the UI can colour it by meaning (provider/model
/// neutral, state by lamp, endpoint muted) rather than by position — the parts
/// differ per backend (BYOK has 3, Ollama 4, Claude Code 2–3).
pub enum Role {
    Provider,
    Model,
    State,
    Endpoint,
}

impl Role {
    fn key(&self) -> &'static str {
        match self {
            Role::Provider => "provider",
            Role::Model => "model",
            Role::State => "state",
            Role::Endpoint => "endpoint",
        }
    }
}

/// One labelled piece of the status chip.
pub struct Segment {
    pub text: String,
    pub role: Role,
}

fn seg(role: Role, text: impl Into<String>) -> Segment {
    Segment {
        text: text.into(),
        role,
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

/// The current AI backend's readiness: `(lamp, segments)`. The segments are the
/// role-tagged chip parts — provider, model, state, and (for Ollama) the endpoint —
/// so the UI can colour each by meaning and the user sees *which* model and *where*
/// at a glance. Does a live Ollama probe only when Ollama is the selected provider;
/// otherwise no network.
pub fn status() -> (Lamp, Vec<Segment>) {
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
                    vec![
                        seg(Role::Provider, "Ollama"),
                        seg(Role::Model, which),
                        seg(Role::State, t("connected", "接続OK")),
                        seg(Role::Endpoint, host),
                    ],
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
                    vec![
                        seg(Role::Provider, "Ollama"),
                        seg(Role::Model, which),
                        seg(Role::State, t("not reachable", "接続できません")),
                        seg(Role::Endpoint, host),
                    ],
                )
            }
        };
    }
    // Claude Code: the `claude` binary is installed and logged in — ready to run.
    // It uses its own default model unless DISKSAGE_MODEL overrides it; show that
    // only when set (we don't guess the default).
    if ai::use_claude_cli() {
        let mut segs = vec![seg(Role::Provider, "Claude Code")];
        if let Some(model) = std::env::var("DISKSAGE_MODEL")
            .ok()
            .filter(|v| !v.is_empty())
        {
            segs.push(seg(Role::Model, model));
        }
        segs.push(seg(Role::State, t("ready", "利用可")));
        return (Lamp::Green, segs);
    }
    // A BYOK HTTP key is present. We haven't called the API (it would cost), so we
    // say "configured", not "connected" — and show the provider + model we'd use.
    if let Ok(p) = ai::resolve_provider() {
        let name = byok_provider_name(&p.url);
        return (
            Lamp::Green,
            vec![
                seg(Role::Provider, name),
                seg(Role::Model, p.model),
                seg(Role::State, t("API key configured", "APIキー設定済み")),
            ],
        );
    }
    // Nothing set up.
    (
        Lamp::Yellow,
        vec![seg(Role::State, t("No AI backend configured", "AI未設定"))],
    )
}

/// JSON payload for `GET /ai-status`:
/// `{"lamp":"green|yellow|red","segments":[{"text":"…","role":"provider|model|state|endpoint"}]}`.
pub fn json() -> String {
    let (lamp, segs) = status();
    let items: Vec<serde_json::Value> = segs
        .iter()
        .map(|s| serde_json::json!({"text": s.text, "role": s.role.key()}))
        .collect();
    serde_json::json!({"lamp": lamp.key(), "segments": items}).to_string()
}

/// The lamp placeholder + the one-shot script that fills it from `/ai-status`.
/// Rendered in the Scan page's AI area; starts as 🟡 "checking…" and updates in
/// place without reloading or scanning. Each `/ai-status` segment becomes its own
/// coloured sub-chip (provider/model neutral, state by lamp, endpoint muted mono),
/// built with `textContent` only — segment text never becomes HTML.
pub fn lamp_html() -> String {
    let checking = t("checking…", "確認中…");
    let html = r#"<div style='margin-bottom:6px'>
<span id='ds-ai-lamp' style='display:inline-flex;align-items:center;flex-wrap:wrap;gap:3px;font-size:12px;color:#1f2328'>🟡 __CHECKING__</span></div>
<script>(function(){var e=document.getElementById('ds-ai-lamp');if(!e)return;
fetch('/ai-status').then(function(r){return r.json();}).then(function(j){
var dot={green:'🟢',yellow:'🟡',red:'🔴'}[j.lamp]||'🟡';
var neutral={provider:{bg:'#ddf4ff',fg:'#0969da'},model:{bg:'#f6f8fa',fg:'#1f2328'},endpoint:{bg:'transparent',fg:'#57606a'}};
var byLamp={green:{bg:'#dafbe1',fg:'#1a7f37'},yellow:{bg:'#fff8c5',fg:'#7d4e00'},red:{bg:'#ffebe9',fg:'#cf222e'}};
e.textContent='';
e.appendChild(document.createTextNode(dot+' '));
(j.segments||[]).forEach(function(s){
var c=s.role==='state'?(byLamp[j.lamp]||byLamp.yellow):(neutral[s.role]||neutral.model);
var span=document.createElement('span');
span.textContent=s.text;
var mono=s.role==='endpoint'?'font-family:ui-monospace,SFMono-Regular,Menlo,monospace;':'';
span.style.cssText='background:'+c.bg+';color:'+c.fg+';border-radius:8px;padding:2px 7px;'+mono;
e.appendChild(span);
});
}).catch(function(){});})();</script>"#;
    html.replace("__CHECKING__", checking)
}

#[cfg(test)]
mod tests {
    use super::{byok_provider_name, host_port, json};

    #[test]
    fn json_is_well_formed() {
        // Whatever the environment, the payload parses, carries a known lamp, and a
        // non-empty list of role-tagged segments.
        let v: serde_json::Value = serde_json::from_str(&json()).expect("valid json");
        let lamp = v["lamp"].as_str().unwrap();
        assert!(matches!(lamp, "green" | "yellow" | "red"), "lamp: {lamp}");
        let segs = v["segments"].as_array().expect("segments is an array");
        assert!(!segs.is_empty(), "at least one segment");
        for s in segs {
            assert!(s["text"].is_string(), "segment has text");
            let role = s["role"].as_str().expect("segment has role");
            assert!(
                matches!(role, "provider" | "model" | "state" | "endpoint"),
                "role: {role}"
            );
        }
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
