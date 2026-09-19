//! Path masking for AI mode — the privacy gate. Before any finding leaves the
//! machine, user-specific path components are anonymized to `<dirN>` while
//! known-safe components (tool dirs, system structure, backup-vendor names) are
//! kept so the model can still judge accurately. Ported faithfully from the bash
//! `ai_mask_findings`. File *contents* are never involved — only paths/metadata.

use std::collections::HashMap;

/// Known-safe, personal-info-free path components (compared lowercased).
const SAFE: &[&str] = &[
    "~", "library", "application support", "application scripts", "containers",
    "group containers", "caches", "cache", "code cache", "gpucache", "developer",
    "xcode", "deriveddata", "data", "vms", "private", "var", "vm", "mobilesync",
    "backup", ".ollama", "models", "blobs", "manifests", ".disksage", "development",
    "documents", "downloads", "desktop", "movies", "music", "pictures", "public",
    "node_modules", "logs", "tmp", "com.docker.docker", "group.com.docker",
    "dockerdesktop", "imobie", "anytrans", "3utools", "dearmob", "imazing",
];

/// Non-personal filenames kept as-is: swapfileN, sleepimage, docker.raw.
fn file_ok(lc: &str) -> bool {
    lc == "sleepimage"
        || lc == "docker.raw"
        || (lc.strip_prefix("swapfile").map(|rest| rest.chars().all(|c| c.is_ascii_digit())) == Some(true))
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(mask_path("/private/var/vm/sleepimage", &mut a), "/private/var/vm/sleepimage");
        assert_eq!(mask_path("/private/var/vm/swapfile3", &mut a), "/private/var/vm/swapfile3");
        assert_eq!(mask_path("/", &mut a), "/");
        assert_eq!(mask_path("~", &mut a), "~");
        assert!(a.is_empty());
    }
}
