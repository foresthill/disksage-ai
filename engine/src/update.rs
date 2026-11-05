//! Signature-free update check: ask GitHub for the newest published release and,
//! if its tag is newer than the running app version, surface a "new version"
//! notice in the UI. There is **no** auto-download and **no** install — only a
//! link to the Releases page.
//!
//! Privacy: this sends nothing about the user. It is a plain unauthenticated GET
//! to the public Releases API, and it is gated by the `update_check` config key
//! (default on) so it can be turned off from Settings. It runs only inside the
//! packaged desktop app — the app exports `DISKSAGE_VERSION`; without it (e.g.
//! the `disksage-engine serve` dev CLI) the check is skipped, since only the app
//! knows which released version it is.

use std::time::Duration;

/// The project repository the released installers come from.
const REPO: &str = "foresthill/disksage-ai";

/// A release newer than the one running.
#[derive(Clone)]
pub struct Latest {
    pub tag: String,
    pub url: String,
}

/// The running app version, or `None` when it is unknown (so no notice is shown).
/// The desktop app exports `DISKSAGE_VERSION` (its bundle version) before it
/// starts the server; the dev CLI does not, and gets no update notice.
pub fn current_version() -> Option<String> {
    std::env::var("DISKSAGE_VERSION")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

/// Is the check turned on? Config key `update_check`, default on; only the exact
/// value `off` disables it.
pub fn enabled() -> bool {
    crate::settings::read_key("update_check") != "off"
}

/// Parse `v0.1.9` / `0.1.9-pre` / `1.2` into `(major, minor, patch)`; any part
/// that is missing or non-numeric counts as 0.
fn parse(tag: &str) -> (u64, u64, u64) {
    let s = tag.trim().trim_start_matches('v');
    let core = s.split(['-', '+']).next().unwrap_or(s);
    let mut it = core.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
    )
}

/// Is `tag` strictly newer than `current`? Compared numerically, so `0.1.10`
/// beats `0.1.9` (a plain string compare would get that wrong).
fn is_newer(tag: &str, current: &str) -> bool {
    parse(tag) > parse(current)
}

/// Ask GitHub for the newest published release. Best-effort: offline, rate-limit,
/// non-2xx or parse failure all return `None`. Honors the `update_check` toggle
/// and only runs when the app version is known.
pub fn check() -> Option<Latest> {
    if !enabled() {
        return None;
    }
    let current = current_version()?;
    let url = format!("https://api.github.com/repos/{REPO}/releases?per_page=10");
    // GitHub requires a User-Agent; without one it answers 403.
    let resp = ureq::get(&url)
        .set("user-agent", "disksage-update-check")
        .set("accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(6))
        .call()
        .ok()?;
    // Read as text + parse (ureq's into_json needs a feature we don't enable;
    // this mirrors ai.rs).
    let text = resp.into_string().ok()?;
    let body: serde_json::Value = serde_json::from_str(&text).ok()?;
    // Releases are returned newest-first; take the first published (non-draft)
    // one — our releases are pre-releases, which the `/latest` endpoint skips.
    let rel = body
        .as_array()?
        .iter()
        .find(|r| r.get("draft").and_then(|d| d.as_bool()) != Some(true))?;
    let tag = rel.get("tag_name")?.as_str()?.to_string();
    let link = rel
        .get("html_url")
        .and_then(|u| u.as_str())
        .unwrap_or("https://github.com/foresthill/disksage-ai/releases")
        .to_string();
    if is_newer(&tag, &current) {
        Some(Latest { tag, url: link })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_detection() {
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(is_newer("0.1.10", "0.1.9")); // numeric, not string, compare
        assert!(!is_newer("v0.1.9", "0.1.9"));
        assert!(!is_newer("v0.1.8", "0.1.9"));
    }

    #[test]
    fn parse_strips_prefix_and_suffix() {
        assert_eq!(parse("v0.1.9"), (0, 1, 9));
        assert_eq!(parse("0.1.9-pre"), (0, 1, 9));
        assert_eq!(parse("1.2"), (1, 2, 0));
        assert_eq!(parse("garbage"), (0, 0, 0));
    }
}
