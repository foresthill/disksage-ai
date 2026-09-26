//! Data-driven scan patterns: the simple "a directory over a size threshold"
//! rules live in `patterns/builtin.json` (bundled at compile time) instead of
//! being hard-coded, so they're easy to read and for the community to extend.
//! Users can drop extra rules in `$DISKSAGE_HOME/patterns.json`.
//!
//! Patterns with bespoke logic (iPhone-backup Manifest.db check, tmutil
//! snapshots, VM swap, Docker.raw, per-app Electron caches, the node_modules
//! aggregate) stay in `scan.rs` — JSON can't express them.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;

use crate::lang::is_ja;

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
const BUILTIN: &str = include_str!("../../patterns/builtin.json");

/// The on-disk JSON shape. `path` is sugar for a single-element `paths`.
#[derive(Deserialize)]
struct PatternJson {
    id: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    paths: Vec<String>,
    threshold_gib: f64,
    severity: String,
    label_en: String,
    label_ja: String,
    action_en: String,
    action_ja: String,
}

/// A compiled pattern. `id`/`severity` are interned to `&'static str` (once, at
/// load) so `Finding` keeps its cheap static fields; labels/actions stay owned.
pub struct Pattern {
    pub id: &'static str,
    pub severity: &'static str,
    pub threshold: u64,
    paths: Vec<String>, // ~-relative candidates, first existing wins
    label_en: String,
    label_ja: String,
    action_en: String,
    action_ja: String,
}

impl Pattern {
    /// First candidate path that exists as a directory, expanded against `home`.
    pub fn first_existing_dir(&self, home: &Path) -> Option<PathBuf> {
        self.paths
            .iter()
            .map(|p| expand(home, p))
            .find(|p| p.is_dir())
    }
    pub fn label(&self) -> &str {
        if is_ja() {
            &self.label_ja
        } else {
            &self.label_en
        }
    }
    pub fn action(&self) -> &str {
        if is_ja() {
            &self.action_ja
        } else {
            &self.action_en
        }
    }
}

fn expand(home: &Path, p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(p),
    }
}

/// Map a JSON severity to one of the known static severities.
fn severity(s: &str) -> &'static str {
    match s {
        "high" => "high",
        "medium" => "medium",
        "low" => "low",
        "safe" => "safe",
        _ => "info",
    }
}

fn compile(j: PatternJson) -> Pattern {
    let mut paths = Vec::new();
    if let Some(p) = j.path {
        paths.push(p);
    }
    paths.extend(j.paths);
    Pattern {
        id: Box::leak(j.id.into_boxed_str()),
        severity: severity(&j.severity),
        threshold: (j.threshold_gib * GIB) as u64,
        paths,
        label_en: j.label_en,
        label_ja: j.label_ja,
        action_en: j.action_en,
        action_ja: j.action_ja,
    }
}

fn parse(text: &str) -> Vec<Pattern> {
    serde_json::from_str::<Vec<PatternJson>>(text)
        .unwrap_or_default()
        .into_iter()
        .map(compile)
        .collect()
}

fn user_file() -> Option<PathBuf> {
    let home = std::env::var_os("DISKSAGE_HOME")
        .map(PathBuf::from)
        .or_else(|| crate::util::home_dir().map(|h| h.join(".disksage")))?;
    let f = home.join("patterns.json");
    f.is_file().then_some(f)
}

/// All compiled patterns (built-in + any user extras), loaded once.
pub fn all() -> &'static [Pattern] {
    static CACHE: OnceLock<Vec<Pattern>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let mut list = parse(BUILTIN);
            if let Some(f) = user_file() {
                if let Ok(txt) = std::fs::read_to_string(&f) {
                    list.extend(parse(&txt));
                }
            }
            list
        })
        .as_slice()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_json_parses_and_has_expected_patterns() {
        let list = parse(BUILTIN);
        assert!(list.len() >= 9, "expected the built-in patterns to load");
        let ids: Vec<&str> = list.iter().map(|p| p.id).collect();
        for id in [
            "ollama_models",
            "npm_cache",
            "pnpm_store",
            "coresimulator_devices",
        ] {
            assert!(ids.contains(&id), "missing built-in pattern: {id}");
        }
        // pnpm has two candidate paths; thresholds convert to bytes.
        let pnpm = list.iter().find(|p| p.id == "pnpm_store").unwrap();
        assert_eq!(pnpm.paths.len(), 2);
        assert_eq!(pnpm.threshold, 5 * 1024 * 1024 * 1024);
        assert_eq!(pnpm.severity, "safe");
    }
}
