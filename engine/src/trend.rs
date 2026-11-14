//! Disk free-space history ("trend") for the served UI. Appends one point per
//! scan to `$DISKSAGE_HOME/history.tsv` and renders it as a dependency-free
//! inline-SVG line chart, so you can actually see free space rise and fall over
//! time — the visual answer to "why does my free space keep shrinking?". Pair it
//! with the snapshot note on the Scan page (see `df::snapshots`).

use std::fs;
use std::path::PathBuf;

use chrono::Local;

use crate::df;
use crate::lang::{is_ja, t};
use crate::util::{esc, home_dir, human};

/// Keep the file small and bounded — this is a trend, not an audit log.
const MAX_POINTS: usize = 500;

fn history_path() -> PathBuf {
    let home = std::env::var_os("DISKSAGE_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|h| h.join(".disksage")))
        .unwrap_or_default();
    home.join("history.tsv")
}

/// The startup container's (free, total) right now — the number people watch.
fn startup_free_total() -> (u64, u64) {
    let groups = df::containers();
    for (total, members) in groups.iter() {
        if members.iter().any(|m| m.mount == "/") {
            let free = members.iter().map(|m| m.avail).max().unwrap_or(0);
            return (free, *total);
        }
    }
    if let Some((total, members)) = groups.iter().next_back() {
        let free = members.iter().map(|m| m.avail).max().unwrap_or(0);
        return (free, *total);
    }
    (0, 0)
}

/// Append one data point (timestamp, free, total). Best-effort: a read-only HOME
/// must never break a scan, so every error is swallowed. Caps the file at
/// `MAX_POINTS` lines (oldest dropped).
pub fn record() {
    let (free, total) = startup_free_total();
    if total == 0 {
        return;
    }
    let ts = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let path = history_path();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let mut lines: Vec<String> = fs::read_to_string(&path)
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default();
    lines.push(format!("{ts}\t{free}\t{total}"));
    let n = lines.len();
    if n > MAX_POINTS {
        lines.drain(0..n - MAX_POINTS);
    }
    let _ = fs::write(&path, lines.join("\n") + "\n");
}

struct Point {
    ts: String,
    free: u64,
}

fn load() -> Vec<Point> {
    fs::read_to_string(history_path())
        .map(|s| s.lines().filter_map(parse_line).collect())
        .unwrap_or_default()
}

// The TSV is `timestamp\tfree\ttotal`; `total` is recorded for future use but the
// chart only needs free, so it's parsed-through (validated) and dropped.
fn parse_line(l: &str) -> Option<Point> {
    let mut it = l.split('\t');
    let ts = it.next()?.to_string();
    let free = it.next()?.parse().ok()?;
    it.next()?.parse::<u64>().ok()?; // total — validated, unused
    Some(Point { ts, free })
}

/// The `/trend` page body: a free-space line chart over time, plus the latest
/// value and the change since the first recorded point.
pub fn trend_html() -> String {
    let heading = format!(
        "<h2 style='font-size:18px;margin:2px 0 4px'>{}</h2>\
         <div style='color:#57606a;font-size:13px;margin-bottom:16px'>{}</div>",
        t("📈 Free-space trend", "📈 空き容量の推移"),
        t(
            "One point is recorded each scan. Watch for a steady fall (something is growing) vs a saw-tooth (snapshots/Trash holding space, then released).",
            "各スキャンで1点を記録します。右肩下がり＝何かが増え続けている／ギザギザ＝スナップショットやゴミ箱が一時的に空きを握り、解放される波です。"
        ),
    );
    let pts = load();
    if pts.is_empty() {
        return format!(
            "{heading}<p style='color:#57606a'>{}</p>",
            t(
                "No history yet — it fills in as you run scans. Open the Scan tab to record the first point.",
                "まだ履歴がありません — スキャンするたびに貯まります。スキャンタブを開くと最初の1点が記録されます。"
            )
        );
    }
    let latest = pts.last().unwrap();
    let first = pts.first().unwrap();
    let delta = latest.free as i128 - first.free as i128;
    let (sign, color) = if delta >= 0 {
        ("+", "#1a7f37")
    } else {
        ("−", "#d1242f")
    };
    let summary = format!(
        "<div style='display:flex;gap:24px;flex-wrap:wrap;margin-bottom:16px'>\
         <div><div style='color:#57606a;font-size:12px'>{}</div>\
         <div style='font-size:20px;font-weight:700'>{}</div></div>\
         <div><div style='color:#57606a;font-size:12px'>{}</div>\
         <div style='font-size:20px;font-weight:700;color:{color}'>{sign}{}</div></div>\
         <div><div style='color:#57606a;font-size:12px'>{}</div>\
         <div style='font-size:20px;font-weight:700'>{}</div></div></div>",
        t("Free now", "現在の空き"),
        human(latest.free),
        t("Change since first point", "最初からの増減"),
        human(delta.unsigned_abs() as u64),
        t("Points recorded", "記録点数"),
        pts.len(),
    );
    let chart = if pts.len() < 2 {
        format!(
            "<p style='color:#57606a'>{}</p>",
            t(
                "Only one point so far — run another scan to draw the line.",
                "まだ1点だけです — もう一度スキャンすると線が描かれます。"
            )
        )
    } else {
        svg_chart(&pts)
    };
    format!("{heading}{summary}{chart}")
}

/// A minimal, dependency-free SVG line chart of free space over the recorded
/// points. The Y axis is scaled to the free-space *range* (not 0..total) so the
/// rise and fall people care about is actually visible rather than a flat line
/// hugging the bottom of a 460 GB disk.
fn svg_chart(pts: &[Point]) -> String {
    let w = 820.0_f64;
    let h = 260.0_f64;
    let (ml, mr, mt, mb) = (64.0_f64, 16.0_f64, 16.0_f64, 34.0_f64);
    let pw = w - ml - mr;
    let ph = h - mt - mb;

    let min = pts.iter().map(|p| p.free).min().unwrap_or(0);
    let max = pts.iter().map(|p| p.free).max().unwrap_or(0);
    // Pad the range so a flat series still has headroom, and so the line never
    // sits exactly on the top/bottom edge.
    let span = (max - min).max(1) as f64;
    let lo = min as f64 - span * 0.15;
    let hi = max as f64 + span * 0.15;
    let range = (hi - lo).max(1.0);

    let n = pts.len();
    let x = |i: usize| ml + pw * (i as f64) / ((n - 1) as f64);
    let y = |free: u64| mt + ph * (1.0 - (free as f64 - lo) / range);

    // Polyline through every point + a filled area under it for readability.
    let mut line = String::new();
    let mut area = format!("{:.1},{:.1} ", ml, mt + ph);
    for (i, p) in pts.iter().enumerate() {
        let (px, py) = (x(i), y(p.free));
        line.push_str(&format!("{px:.1},{py:.1} "));
        area.push_str(&format!("{px:.1},{py:.1} "));
    }
    area.push_str(&format!("{:.1},{:.1}", ml + pw, mt + ph));

    // Y gridlines + labels at hi / mid / lo (shown as free-space figures).
    let mut grid = String::new();
    for (frac, val) in [(0.0, hi), (0.5, (hi + lo) / 2.0), (1.0, lo)] {
        let gy = mt + ph * frac;
        grid.push_str(&format!(
            "<line x1='{ml:.1}' y1='{gy:.1}' x2='{:.1}' y2='{gy:.1}' stroke='#eaeef2' stroke-width='1'/>\
             <text x='{:.1}' y='{:.1}' font-size='11' fill='#57606a' text-anchor='end'>{}</text>",
            ml + pw,
            ml - 8.0,
            gy + 4.0,
            esc(&human(val.max(0.0) as u64)),
        ));
    }

    // Latest point marker.
    let (lx, ly) = (x(n - 1), y(pts[n - 1].free));

    // X labels: first and last timestamps (date + time is long, show as-is).
    let x_first = &first_ts(pts);
    let x_last = &pts[n - 1].ts;

    format!(
        "<svg viewBox='0 0 {w:.0} {h:.0}' width='100%' style='max-width:{w:.0}px;height:auto;\
         border:1px solid #d0d7de;border-radius:10px;background:#fff'>\
         {grid}\
         <polyline points='{area}' fill='#0969da' fill-opacity='0.08' stroke='none'/>\
         <polyline points='{line}' fill='none' stroke='#0969da' stroke-width='2' \
         stroke-linejoin='round' stroke-linecap='round'/>\
         <circle cx='{lx:.1}' cy='{ly:.1}' r='3.5' fill='#0969da'/>\
         <text x='{ml:.1}' y='{tb:.1}' font-size='11' fill='#57606a' text-anchor='start'>{xf}</text>\
         <text x='{xr:.1}' y='{tb:.1}' font-size='11' fill='#57606a' text-anchor='end'>{xl}</text>\
         </svg>\
         <div style='color:#57606a;font-size:12px;margin-top:8px'>{note}</div>",
        tb = h - 12.0,
        xr = ml + pw,
        xf = esc(x_first),
        xl = esc(x_last),
        note = if is_ja() {
            "縦軸＝起動ディスクの空き容量（表示範囲は増減が見えるよう自動ズーム）。横軸＝時間。"
        } else {
            "Y = startup-disk free space (auto-zoomed to the range so changes are visible). X = time."
        },
    )
}

fn first_ts(pts: &[Point]) -> String {
    pts.first().map(|p| p.ts.clone()).unwrap_or_default()
}
