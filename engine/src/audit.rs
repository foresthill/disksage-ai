//! AI-mode audit log — ported from the bash `--ai-log` / `DISKSAGE_AI_LOG=1`.
//!
//! When enabled, every AI send drops a timestamped folder under
//! `$DISKSAGE_HOME/ai-logs/<stamp>/` holding exactly what left (and returned to)
//! the machine, so the privacy guarantee is auditable after the fact:
//!   - `request.json`  — the masked request body actually POSTed (metadata only)
//!   - `response.json` — the raw provider response
//!   - `masking.tsv`   — the real→masked path table (real paths are included on
//!     purpose; this file is LOCAL-only, never shared)
//!
//! The masking table is recomputed with the same deterministic aliasing
//! `ai::build_request` uses (fresh `Aliases`, same finding order), so it is a
//! faithful record of what the model saw — and lets us spot over-masking.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::Local;

use crate::mask;
use crate::scan::Finding;
use crate::util::home_dir;

/// `$DISKSAGE_HOME/ai-logs` (default `~/.disksage/ai-logs`).
pub fn logs_dir() -> PathBuf {
    let home = std::env::var_os("DISKSAGE_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|h| h.join(".disksage")))
        .unwrap_or_default();
    home.join("ai-logs")
}

/// True when audit logging is on: the `--ai-log` flag, or `DISKSAGE_AI_LOG=1`.
pub fn enabled(flag: bool) -> bool {
    flag || std::env::var("DISKSAGE_AI_LOG").ok().as_deref() == Some("1")
}

/// One audit session = one timestamped directory. Created eagerly by `start`.
pub struct Audit {
    dir: PathBuf,
}

/// Create the timestamped audit directory for this send.
pub fn start() -> io::Result<Audit> {
    let stamp = Local::now().format("%Y-%m-%d_%H%M%S").to_string();
    let dir = logs_dir().join(stamp);
    fs::create_dir_all(&dir)?;
    Ok(Audit { dir })
}

impl Audit {
    /// The directory holding this session's files (for a "saved to …" message).
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The exact request body sent (already masked by `build_request`).
    pub fn write_request(&self, body: &str) -> io::Result<()> {
        fs::write(self.dir.join("request.json"), body)
    }

    /// The raw provider response (success body or the JSON error body).
    pub fn write_response(&self, raw: &str) -> io::Result<()> {
        fs::write(self.dir.join("response.json"), raw)
    }

    /// The real→masked path table, matching what was actually sent.
    pub fn write_masking(&self, findings: &[Finding]) -> io::Result<()> {
        fs::write(self.dir.join("masking.tsv"), masking_tsv(findings))
    }
}

/// Build the `masking.tsv` body: one row per finding, `real \t masked \t
/// anonymized`, using the same fresh, ordered aliasing as `build_request` so the
/// masked column equals what the model received.
///
/// Paths containing a literal tab/newline (legal but vanishingly rare) would blur
/// the columns; acceptable for a local diagnostic file.
pub fn masking_tsv(findings: &[Finding]) -> String {
    let mut aliases = mask::Aliases::new();
    let mut tsv = String::from("real\tmasked\tanonymized\n");
    for f in findings {
        let masked = mask::mask_path(&f.path, &mut aliases);
        let anonymized = if masked != f.path { "yes" } else { "no" };
        tsv.push_str(&format!("{}\t{}\t{}\n", f.path, masked, anonymized));
    }
    tsv
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(id: &'static str, path: &str) -> Finding {
        Finding {
            id,
            path: path.to_string(),
            size: 1024,
            severity: "low",
            description: "example".to_string(),
            action: "example".to_string(),
        }
    }

    #[test]
    fn tsv_records_real_and_masked_with_matching_aliases() {
        let f = vec![
            finding(
                "node_modules_aggregate",
                "~/Development/Secret/node_modules",
            ),
            finding("ollama_models", "~/.ollama/models"),
        ];
        let tsv = masking_tsv(&f);
        assert!(tsv.starts_with("real\tmasked\tanonymized\n"));
        // Anonymized row: real name kept locally, masked column carries <dir1>.
        assert!(tsv
            .contains("~/Development/Secret/node_modules\t~/Development/<dir1>/node_modules\tyes"));
        // Safe path: unchanged, flagged "no".
        assert!(tsv.contains("~/.ollama/models\t~/.ollama/models\tno"));
    }

    #[test]
    fn masked_column_never_leaks_the_real_name() {
        let f = vec![finding(
            "node_modules_aggregate",
            "~/Development/SecretProject/node_modules",
        )];
        let tsv = masking_tsv(&f);
        // The masked half of the mapping must not contain the real user name.
        let masked_col = tsv
            .lines()
            .nth(1)
            .and_then(|l| l.split('\t').nth(1))
            .unwrap_or("");
        assert!(
            !masked_col.contains("SecretProject"),
            "masked column leaked a real name"
        );
        assert!(masked_col.contains("<dir1>"));
    }
}
