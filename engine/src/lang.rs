//! UI language for the served pages. Resolved once at serve startup with the
//! same precedence as the bash CLI, then a tiny `t(en, ja)` picks strings.
//!
//! Scope note: this covers the UI *chrome* (nav, headings, buttons, settings).
//! Finding descriptions/actions still come from the engine's scan in English —
//! translating those needs an id-keyed catalog (like the bash version has) and
//! is the remaining i18n step before the desktop should switch to this server.

use std::sync::atomic::{AtomicBool, Ordering};

/// The resolved language. An atomic (not a `OnceLock`) so a language change in
/// the settings UI takes effect immediately, without restarting the server —
/// the old startup-fixed behaviour was the "I switched to 日本語 but it stayed
/// English until restart" surprise.
static JA: AtomicBool = AtomicBool::new(false);

fn starts_ja(s: &str) -> bool {
    s.trim().to_lowercase().starts_with("ja")
}

/// Precedence: DISKSAGE_LANG env → config `lang=` → $LC_ALL/$LANG → the OS UI
/// locale (cross-platform) → English. The OS-locale step means a Japanese
/// Windows or Linux (not just macOS) auto-defaults to Japanese, instead of
/// surprising the user with English.
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
    // OS UI locale, e.g. "ja-JP" — works on macOS, Windows and Linux.
    if let Some(loc) = sys_locale::get_locale() {
        return starts_ja(&loc);
    }
    false
}

/// Resolve the language from env/config/locale (call at serve startup, and again
/// via `refresh` after the user changes the setting).
pub fn init() {
    JA.store(detect(), Ordering::Relaxed);
}

/// Re-resolve after a settings change so the new choice applies to the very next
/// request (no restart needed).
pub fn refresh() {
    init();
}

pub fn is_ja() -> bool {
    JA.load(Ordering::Relaxed)
}

/// Pick the English or Japanese variant of a static string.
pub fn t(en: &'static str, ja: &'static str) -> &'static str {
    if is_ja() {
        ja
    } else {
        en
    }
}
