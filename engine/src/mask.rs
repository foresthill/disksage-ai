//! Path masking for AI mode — the privacy gate. Before any finding leaves the
//! machine, user-specific path components are anonymized to `<dirN>` while
//! known-safe components (tool dirs, system structure, backup-vendor names) are
//! kept so the model can still judge accurately. Ported faithfully from the bash
//! `ai_mask_findings`. File *contents* are never involved — only paths/metadata.

use std::collections::HashMap;

/// Known-safe, personal-info-free path components (compared lowercased).
const SAFE: &[&str] = &[
    "~",
    "library",
    "application support",
    "application scripts",
    "containers",
    "group containers",
    "caches",
    "cache",
    "code cache",
    "gpucache",
    "developer",
    "xcode",
    "deriveddata",
    "data",
    "vms",
    "private",
    "var",
    "vm",
    "mobilesync",
    "backup",
    ".ollama",
    "models",
    "blobs",
    "manifests",
    ".disksage",
    "development",
    "documents",
    "downloads",
    "desktop",
    "movies",
    "music",
    "pictures",
    "public",
    "node_modules",
    "logs",
    "tmp",
    "com.docker.docker",
    "group.com.docker",
    "dockerdesktop",
    "imobie",
    "anytrans",
    "3utools",
    "dearmob",
    "imazing",
];

/// Non-personal filenames kept as-is: swapfileN, sleepimage, docker.raw.
fn file_ok(lc: &str) -> bool {
    lc == "sleepimage"
        || lc == "docker.raw"
        || (lc
            .strip_prefix("swapfile")
            .map(|rest| rest.chars().all(|c| c.is_ascii_digit()))
            == Some(true))
}

fn is_safe(c: &str) -> bool {
    let lc = c.to_lowercase();
    SAFE.contains(&lc.as_str())
        || (!c.is_empty() && c.chars().all(|ch| ch.is_ascii_digit()))
        || file_ok(&lc)
}

/// Alias table stable within one run: the same user name always maps to the same
/// `<dirN>`, so the model still sees structure without seeing the real names.
pub type Aliases = HashMap<String, String>;

fn mask_component(c: &str, aliases: &mut Aliases) -> String {
    if c.is_empty() || is_safe(c) {
        return c.to_string();
    }
    let key = c.to_lowercase();
    if let Some(a) = aliases.get(&key) {
        return a.clone();
    }
    let a = format!("<dir{}>", aliases.len() + 1);
    aliases.insert(key, a.clone());
    a
}

/// Mask a (tildified) path: keep `/` and `~`, anonymize user-specific components.
pub fn mask_path(path: &str, aliases: &mut Aliases) -> String {
    if path.is_empty() || path == "/" || path == "~" {
        return path.to_string();
    }
    path.split('/')
        .map(|c| mask_component(c, aliases))
        .collect::<Vec<_>>()
        .join("/")
}

/// Mask free text (e.g. a finding's description) using the SAME aliases already
/// built from the paths, so a user/app name that was anonymized in the path
/// (e.g. `Claude` → `<dir2>`) is also replaced wherever it appears in the text.
/// Case-insensitive; alias keys are ASCII-ish component names so byte indices
/// line up with the lowercased copy.
pub fn mask_text(text: &str, aliases: &Aliases) -> String {
    let mut out = text.to_string();
    for (real_lc, token) in aliases {
        out = replace_ci(&out, real_lc, token);
    }
    out
}

fn replace_ci(haystack: &str, needle_lc: &str, replacement: &str) -> String {
    if needle_lc.is_empty() {
        return haystack.to_string();
    }
    let hay_lc = haystack.to_lowercase();
    // If lowercasing changed the byte length, indices would misalign — fall back
    // to a conservative exact-case replace rather than risk corrupting the text.
    if hay_lc.len() != haystack.len() {
        return haystack.replace(needle_lc, replacement);
    }
    let mut result = String::with_capacity(haystack.len());
    let mut last = 0;
    while let Some(rel) = hay_lc[last..].find(needle_lc) {
        let abs = last + rel;
        result.push_str(&haystack[last..abs]);
        result.push_str(replacement);
        last = abs + needle_lc.len();
    }
    result.push_str(&haystack[last..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_text_scrubs_names_from_descriptions() {
        let mut a = Aliases::new();
        // Masking the path registers Claude → <dir1>.
        assert_eq!(
            mask_path("~/Library/Application Support/Claude", &mut a),
            "~/Library/Application Support/<dir1>"
        );
        // The same alias scrubs the app name out of the description (any case).
        assert_eq!(
            mask_text("Claude の Electron キャッシュ: 1.0 GB", &a),
            "<dir1> の Electron キャッシュ: 1.0 GB"
        );
        assert!(!mask_text("claude cache", &a).contains("claude"));
    }

    #[test]
    fn keeps_known_components() {
        let mut a = Aliases::new();
        assert_eq!(mask_path("~/Library/Caches", &mut a), "~/Library/Caches");
        assert_eq!(mask_path("~/Development", &mut a), "~/Development");
        assert_eq!(
            mask_path("~/.ollama/models/blobs", &mut a),
            "~/.ollama/models/blobs"
        );
        assert!(a.is_empty(), "no user-specific components → no aliases");
    }

    #[test]
    fn anonymizes_user_dirs_stably() {
        let mut a = Aliases::new();
        // "MyProject" and "Secret" are user-specific → <dir1>, <dir2>; digits kept.
        assert_eq!(
            mask_path("~/Development/MyProject/node_modules", &mut a),
            "~/Development/<dir1>/node_modules"
        );
        assert_eq!(
            mask_path("~/Development/Secret/node_modules", &mut a),
            "~/Development/<dir2>/node_modules"
        );
        // Same name → same alias (stable within the run).
        assert_eq!(
            mask_path("~/Documents/MyProject", &mut a),
            "~/Documents/<dir1>"
        );
    }

    #[test]
    fn keeps_special_files_and_passthrough() {
        let mut a = Aliases::new();
        assert_eq!(
            mask_path("/private/var/vm/sleepimage", &mut a),
            "/private/var/vm/sleepimage"
        );
        assert_eq!(
            mask_path("/private/var/vm/swapfile3", &mut a),
            "/private/var/vm/swapfile3"
        );
        assert_eq!(mask_path("/", &mut a), "/");
        assert_eq!(mask_path("~", &mut a), "~");
        assert!(a.is_empty());
    }
}
