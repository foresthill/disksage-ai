//! UI language for the served pages. Resolved once at serve startup with the
//! same precedence as the bash CLI, then a tiny `t(en, ja)` picks strings.
//!
//! Scope note: this covers the UI *chrome* (nav, headings, buttons, settings).
//! Finding descriptions/actions still come from the engine's scan in English —
//! translating those needs an id-keyed catalog (like the bash version has) and
//! is the remaining i18n step before the desktop should switch to this server.

use std::sync::OnceLock;

static JA: OnceLock<bool> = OnceLock::new();

fn starts_ja(s: &str) -> bool {
    s.trim().to_lowercase().starts_with("ja")
}

/// Precedence: DISKSAGE_LANG env → config `lang=` → $LC_ALL/$LANG → macOS
/// AppleLocale → English. Mirrors the bash `ai_lang`.
fn detect() -> bool {
    if let Ok(v) = std::env::var("DISKSAGE_LANG") {
        if !v.is_empty() {
            return starts_ja(&v);
        }
    }
    let cfg = crate::settings::read_lang();
    if !cfg.is_empty() {
        return starts_ja(&cfg);
    }
    for k in ["LC_ALL", "LANG"] {
        if let Ok(v) = std::env::var(k) {
            if !v.is_empty() {
                return starts_ja(&v);
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("defaults")
            .args(["read", "-g", "AppleLocale"])
            .output()
        {
            if out.status.success() {
                return starts_ja(&String::from_utf8_lossy(&out.stdout));
            }
        }
    }
    false
}

/// Resolve the language once (call at serve startup).
pub fn init() {
    let _ = JA.set(detect());
}

pub fn is_ja() -> bool {
    *JA.get().unwrap_or(&false)
}

/// Pick the English or Japanese variant of a static string.
pub fn t(en: &'static str, ja: &'static str) -> &'static str {
    if is_ja() {
        ja
    } else {
        en
    }
}
