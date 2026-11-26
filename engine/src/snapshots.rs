//! `snapshots` diagnostic — list and classify local APFS / Time Machine
//! snapshots, flag the "pinned" ones that quietly hold disk space, and advise
//! what to do (never deleting anything automatically — DiskSage only proposes).
//!
//! Why this exists: a snapshot pins recently-deleted blocks, so "I deleted files
//! but free space didn't come back" is almost always a snapshot holding them. The
//! two kinds that matter are Time Machine *local* snapshots (made automatically
//! when "Back Up Automatically" is on, even with no external drive) and the
//! `com.apple.os.update-*` snapshot staged during a macOS update. The latter is a
//! common *hidden* holder and must NOT be deleted by hand — it clears when the
//! update finishes.
//!
//! Read-only. macOS only; other OSes report n/a. Sources (verified 2026-11-18):
//! Apple Mac User Guide <https://support.apple.com/guide/mac-help/mh35933/mac>,
//! `man tmutil` (thinlocalsnapshots / deletelocalsnapshots / listlocalsnapshots).

use std::fmt::Write as _;

use crate::lang::t;

/// What created a snapshot — decides the advice and whether it's safe to touch.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Kind {
    /// `com.apple.TimeMachine.*` — safe to thin; macOS also auto-thins these.
    TimeMachine,
    /// `com.apple.os.update-*` — staged OS update. Never delete by hand.
    OsUpdate,
    /// Anything else (third-party backup/VM tooling). Managed by that app.
    Other,
}

impl Kind {
    fn classify(name: &str) -> Kind {
        if name.contains("com.apple.TimeMachine") {
            Kind::TimeMachine
        } else if name.contains("os.update") {
            Kind::OsUpdate
        } else {
            Kind::Other
        }
    }

    /// Stable machine key for JSON output.
    pub fn key(&self) -> &'static str {
        match self {
            Kind::TimeMachine => "time_machine",
            Kind::OsUpdate => "os_update",
            Kind::Other => "other",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Kind::TimeMachine => t("Time Machine (local)", "Time Machine（ローカル）"),
            Kind::OsUpdate => t("macOS update (staged)", "macOS更新（準備物）"),
            Kind::Other => t("Other / third-party", "その他・サードパーティ"),
        }
    }
}

/// One snapshot and the two things we care about: what made it, and whether it is
/// "pinned" (Purgeable: No, or the diskutil note that it limits the minimum
/// container size) — a pinned snapshot holds space that freeing files won't return.
pub struct Snap {
    pub name: String,
    pub kind: Kind,
    pub pinned: bool,
}

/// Parse `diskutil apfs listSnapshots /` output into classified snapshots. A
/// snapshot block starts at its `Name:` line; the following `Purgeable:` / `NOTE:`
/// lines belong to that most-recent snapshot. Kept pure (text in, data out) so it
/// is unit-testable without running diskutil.
pub fn parse_diskutil(text: &str) -> Vec<Snap> {
    let mut snaps: Vec<Snap> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("Name:") {
            let name = name.trim().to_string();
            let kind = Kind::classify(&name);
            snaps.push(Snap {
                name,
                kind,
                pinned: false,
            });
        } else if let Some(p) = line.strip_prefix("Purgeable:") {
            if p.trim().eq_ignore_ascii_case("No") {
                if let Some(last) = snaps.last_mut() {
                    last.pinned = true;
                }
            }
        } else if line.starts_with("NOTE:") && line.contains("limits the minimum size") {
            if let Some(last) = snaps.last_mut() {
                last.pinned = true;
            }
        }
    }
    snaps
}

/// Parse `tmutil listlocalsnapshots /` — one `com.apple.TimeMachine.*` name per
/// line (plus a "Snapshots for disk /:" header we ignore).
pub fn parse_tmutil(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| l.starts_with("com.apple.TimeMachine"))
        .map(String::from)
        .collect()
}

/// Run both tools on macOS. `None` off macOS or if diskutil isn't usable.
#[cfg(target_os = "macos")]
fn gather() -> Option<(Vec<Snap>, Vec<String>)> {
    use std::process::Command;
    let d = Command::new("diskutil")
        .args(["apfs", "listSnapshots", "/"])
        .output()
        .ok()?;
    let apfs = if d.status.success() {
        parse_diskutil(&String::from_utf8_lossy(&d.stdout))
    } else {
        return None;
    };
    // tmutil is best-effort: its absence just means "no TM local list".
    let tm = Command::new("tmutil")
        .args(["listlocalsnapshots", "/"])
        .output()
        .ok()
        .map(|o| parse_tmutil(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default();
    Some((apfs, tm))
}

#[cfg(not(target_os = "macos"))]
fn gather() -> Option<(Vec<Snap>, Vec<String>)> {
    None
}

fn count(snaps: &[Snap], k: Kind) -> usize {
    snaps.iter().filter(|s| s.kind == k).count()
}

/// Human-readable report with per-kind advice. Respects the resolved UI language.
fn human(apfs: &[Snap], tm: &[String]) {
    print!("{}", build_human(apfs, tm));
}

/// Build the per-kind report as a String (so the CLI can print it and the MCP
/// server can return it). Respects the resolved UI language via `t()`.
fn build_human(apfs: &[Snap], tm: &[String]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "## {}\n",
        t("Snapshot diagnosis", "スナップショット診断")
    );
    if apfs.is_empty() && tm.is_empty() {
        let _ = writeln!(
            out,
            "{}",
            t(
                "No local snapshots. Snapshots are not holding any of your free space.",
                "ローカルスナップショットはありません。空き容量を縛っているスナップショットはありません。"
            )
        );
        return out;
    }
    let pinned = apfs.iter().filter(|s| s.pinned).count();
    let _ = writeln!(
        out,
        "{} {} · {} {} · ⚠️ {} {}\n",
        apfs.len(),
        t("APFS snapshot(s)", "APFSスナップショット"),
        tm.len(),
        t("Time Machine local", "Time Machineローカル"),
        pinned,
        t(
            "pinned (holding space that freeing files won't return)",
            "pinned（ファイルを消しても戻らない容量を保持）"
        )
    );
    for s in apfs {
        let flag = if s.pinned { " ⚠️ pinned" } else { "" };
        let _ = writeln!(out, "- [{}] {}{}", s.kind.label(), s.name, flag);
    }

    let _ = writeln!(out, "\n### {}", t("What to do", "対処"));
    if count(apfs, Kind::OsUpdate) > 0 {
        let _ = writeln!(
            out,
            "- {} — {}",
            t("macOS update (staged)", "macOS更新（準備物）"),
            t(
                "leave it; it clears automatically when the update finishes. Do NOT delete it by hand.",
                "触らない。OS更新が完了すれば自動で消える。手で削除しないこと。"
            )
        );
    }
    if !tm.is_empty() || count(apfs, Kind::TimeMachine) > 0 {
        let _ = writeln!(
            out,
            "- {} — {}\n  `sudo tmutil thinlocalsnapshots / 10000000000 4`",
            t("Time Machine (local)", "Time Machine（ローカル）"),
            t(
                "safe to thin; macOS also auto-thins after ~24h or under space pressure. To reclaim now:",
                "間引いてよい（macOSも約24h or 空き逼迫で自動間引き）。今すぐ回収するなら:"
            )
        );
    }
    if count(apfs, Kind::Other) > 0 {
        let _ = writeln!(
            out,
            "- {} — {}",
            t("Other / third-party", "その他・サードパーティ"),
            t(
                "managed by the app that created it; remove it from that app.",
                "作成したアプリの管理下。そのアプリから整理する。"
            )
        );
    }
    let _ = writeln!(
        out,
        "\n_{}_",
        t(
            "DiskSage never deletes snapshots for you — it only shows and advises.",
            "DiskSageはスナップショットを自動削除しません。表示と助言のみです。"
        )
    );
    out
}

/// A read-only snapshot diagnosis as text, for the MCP server. macOS only; other
/// platforms get an n/a line. Does not touch the global locale (MCP stays English).
pub fn report_text() -> String {
    match gather() {
        Some((apfs, tm)) => build_human(&apfs, &tm),
        None => t(
            "Snapshot diagnosis is macOS-only (APFS/Time Machine); n/a on this platform.",
            "スナップショット診断はmacOS専用（APFS/Time Machine）。このOSでは非対応です。",
        )
        .to_string(),
    }
}

/// Dependency-free JSON (matches the `df` command's hand-rolled style).
fn json(apfs: &[Snap], tm: &[String], platform: &str) {
    let items: String = apfs
        .iter()
        .map(|s| {
            format!(
                "{{\"name\":\"{}\",\"kind\":\"{}\",\"pinned\":{}}}",
                s.name.replace('"', "\\\""),
                s.kind.key(),
                s.pinned
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let pinned = apfs.iter().filter(|s| s.pinned).count();
    println!(
        "{{\"platform\":\"{}\",\"apfs\":[{}],\"time_machine\":{},\"counts\":{{\"time_machine\":{},\"os_update\":{},\"other\":{},\"pinned\":{}}}}}",
        platform,
        items,
        tm.len(),
        count(apfs, Kind::TimeMachine),
        count(apfs, Kind::OsUpdate),
        count(apfs, Kind::Other),
        pinned
    );
}

/// `snapshots [--json]` entry point.
pub fn run(as_json: bool) {
    crate::lang::init();
    match gather() {
        Some((apfs, tm)) => {
            if as_json {
                json(&apfs, &tm, "macos");
            } else {
                human(&apfs, &tm);
            }
        }
        None => {
            if as_json {
                println!("{{\"platform\":\"other\",\"apfs\":[],\"time_machine\":0,\"counts\":{{\"time_machine\":0,\"os_update\":0,\"other\":0,\"pinned\":0}}}}");
            } else {
                println!(
                    "{}",
                    t(
                        "Snapshot diagnosis is macOS-only (APFS/Time Machine); n/a on this platform.",
                        "スナップショット診断はmacOS専用（APFS/Time Machine）。このOSでは非対応です。"
                    )
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISKUTIL_SAMPLE: &str = "\
Snapshot for disk3s1s1 (1 found)
|
+-- 7DD67C29-82AB-4282-9B50-3FAB25396156
    Name:        com.apple.os.update-60587424F5399FC05D957DF05B4D2F65543462495C77AEF520F790FBB57CB212
    XID:         23858285
    Purgeable:   No
    NOTE:        This snapshot limits the minimum size of APFS Container disk3";

    #[test]
    fn classifies_and_flags_os_update_as_pinned() {
        let snaps = parse_diskutil(DISKUTIL_SAMPLE);
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].kind, Kind::OsUpdate);
        assert!(snaps[0].pinned, "Purgeable: No / NOTE => pinned");
    }

    #[test]
    fn classifies_time_machine_and_purgeable_yes_is_not_pinned() {
        let text = "\
Snapshot for disk3s1s1 (1 found)
+-- AAAA
    Name:        com.apple.TimeMachine.2026-11-18-120000.local
    Purgeable:   Yes";
        let snaps = parse_diskutil(text);
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].kind, Kind::TimeMachine);
        assert!(!snaps[0].pinned);
    }

    #[test]
    fn parse_tmutil_keeps_only_snapshot_names() {
        let text = "Snapshots for disk /:\ncom.apple.TimeMachine.2026-11-18-120000.local\n";
        let got = parse_tmutil(text);
        assert_eq!(got, vec!["com.apple.TimeMachine.2026-11-18-120000.local"]);
    }

    #[test]
    fn empty_tmutil_is_zero() {
        assert!(parse_tmutil("Snapshots for disk /:\n").is_empty());
    }
}
