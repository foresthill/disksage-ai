//! Disk free-space history ("trend") for the served UI. Appends one point per
//! scan to `$DISKSAGE_HOME/history.tsv` and renders it as a dependency-free
//! inline-SVG line chart, so you can actually see free space rise and fall over
//! time — the visual answer to "why does my free space keep shrinking?". Pair it
//! with the snapshot note on the Scan page (see `df::snapshots`).

use std::fs;
use std::path::PathBuf;

use chrono::{Duration, Local, NaiveDateTime};

use crate::df;
use crate::lang::{is_ja, t};
use crate::util::{esc, home_dir, human};

/// Absolute safety cap on stored points, independent of retention — bounds the
/// file even at a fine interval with "forever" retention (100k lines ≈ a few MB).
/// Normal pruning is by age (see `retention_days`); this is just a backstop.
pub const MAX_POINTS: usize = 100_000;

/// Approximate bytes per stored line — `YYYY-MM-DD HH:MM:SS\t<free>\t<total>\t<kind>\n`
/// is ~53 bytes at most; rounded up. Used only for the Settings storage estimate.
pub const BYTES_PER_POINT: u64 = 56;

/// Upper bound on points actually drawn in the chart, so a long "forever" history
/// stays light to render and legible rather than thousands of pixels of mush.
const PLOT_CAP: usize = 800;

/// Default periodic-sampling interval (minutes) when the config key is unset.
const DEFAULT_INTERVAL_MIN: u64 = 30;

fn history_path() -> PathBuf {
    let home = std::env::var_os("DISKSAGE_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|h| h.join(".disksage")))
        .unwrap_or_default();
    home.join("history.tsv")
}

/// How often the background sampler should record a point, in minutes.
/// Reads config `trend_interval`: unset → default (30), "off" → None (disabled),
/// "15"/"30"/"60" (or any positive integer) → that many minutes.
pub fn interval_minutes() -> Option<u64> {
    match crate::settings::read_key("trend_interval").as_str() {
        "" => Some(DEFAULT_INTERVAL_MIN),
        "off" => None,
        other => other.parse::<u64>().ok().filter(|m| *m > 0),
    }
}

/// How far back to keep history, in days. Reads config `trend_retention`: unset or
/// "forever" → None (keep everything, bounded only by `MAX_POINTS`); a positive
/// integer → that many days. Forever is the default because even the finest
/// interval tops out at a few MB (see `MAX_POINTS` / `BYTES_PER_POINT`). The
/// interval sets *resolution*; this sets *how far back you can look* — independent.
pub fn retention_days() -> Option<u64> {
    match crate::settings::read_key("trend_retention").as_str() {
        "" | "forever" => None,
        other => other.parse::<u64>().ok().filter(|d| *d > 0),
    }
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

/// Append one data point (timestamp, free, total, kind). `scan=true` marks a
/// point taken because the user ran a scan (a special moment, shown emphasised on
/// the chart); `scan=false` is a routine periodic sample from the background
/// watcher. Best-effort: a read-only HOME must never break a scan, so every error
/// is swallowed. Caps the file at `MAX_POINTS` lines (oldest dropped).
pub fn record(scan: bool) {
    let (free, total) = startup_free_total();
    if total == 0 {
        return;
    }
    let ts = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let kind = if scan { "scan" } else { "auto" };
    let path = history_path();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let mut lines: Vec<String> = fs::read_to_string(&path)
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default();
    lines.push(format!("{ts}\t{free}\t{total}\t{kind}"));
    // Primary pruning is by age (retention). A point whose timestamp can't be
    // parsed is kept (never drop data on a parse hiccup).
    if let Some(days) = retention_days() {
        let cutoff = Local::now().naive_local() - Duration::days(days as i64);
        lines.retain(|l| match l.split('\t').next().and_then(parse_ts) {
            Some(t) => t >= cutoff,
            None => true,
        });
    }
    // Hard safety backstop regardless of retention.
    let n = lines.len();
    if n > MAX_POINTS {
        lines.drain(0..n - MAX_POINTS);
    }
    let _ = fs::write(&path, lines.join("\n") + "\n");
}

/// Parse a stored timestamp (`%Y-%m-%d %H:%M:%S`, local time) for age pruning.
fn parse_ts(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()
}

#[derive(Clone)]
struct Point {
    ts: String,
    free: u64,
    scan: bool,
}

/// Thin the series for plotting when it is longer than `PLOT_CAP`, but never drop
/// a scan (◆) point or the most recent one — those are the points people look for.
fn downsample(pts: &[Point]) -> Vec<Point> {
    if pts.len() <= PLOT_CAP {
        return pts.to_vec();
    }
    let stride = pts.len().div_ceil(PLOT_CAP);
    let last = pts.len() - 1;
    pts.iter()
        .enumerate()
        .filter(|(i, p)| p.scan || i % stride == 0 || *i == last)
        .map(|(_, p)| p.clone())
        .collect()
}

fn load() -> Vec<Point> {
    fs::read_to_string(history_path())
        .map(|s| s.lines().filter_map(parse_line).collect())
        .unwrap_or_default()
}

// The TSV is `timestamp\tfree\ttotal[\tkind]`; `total` is recorded for future use
// but the chart only needs free, so it's parsed-through (validated) and dropped.
// The 4th `kind` column is optional: legacy 3-column lines were all written at
// scan time, so a missing kind is treated as `scan`.
fn parse_line(l: &str) -> Option<Point> {
    let mut it = l.split('\t');
    let ts = it.next()?.to_string();
    let free = it.next()?.parse().ok()?;
    it.next()?.parse::<u64>().ok()?; // total — validated, unused
    let scan = !matches!(it.next(), Some("auto"));
    Some(Point { ts, free, scan })
}

/// The `/trend` page body: a free-space line chart over time, plus the latest
/// value and the change since the first recorded point.
pub fn trend_html() -> String {
    let heading = format!(
        "<h2 style='font-size:18px;margin:2px 0 4px'>{}</h2>\
         <div style='color:#57606a;font-size:13px;margin-bottom:16px'>{}</div>",
        t("📈 Free-space trend", "📈 空き容量の推移"),
        t(
            "Points are sampled periodically in the background (while DiskSage is running), plus a special ◆ point each time you scan. Watch for a steady fall (something is growing) vs a saw-tooth (snapshots/Trash holding space, then released — e.g. space freed overnight).",
            "DiskSage 常駐中はバックグラウンドで定期的に点を記録し、スキャンした時はその瞬間を ◆ で特別表示します。右肩下がり＝何かが増え続けている／ギザギザ＝スナップショットやゴミ箱が一時的に空きを握り解放される波（夜間に空きが戻る現象など）。"
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
        svg_chart(&downsample(&pts))
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
    // Each point also gets an invisible hit-circle carrying the exact figures, so
    // hovering shows a tooltip with the precise free space and timestamp.
    let mut line = String::new();
    let mut area = format!("{:.1},{:.1} ", ml, mt + ph);
    let mut hits = String::new();
    for (i, p) in pts.iter().enumerate() {
        let (px, py) = (x(i), y(p.free));
        line.push_str(&format!("{px:.1},{py:.1} "));
        area.push_str(&format!("{px:.1},{py:.1} "));
        // Tooltip lines: timestamp (marked when it's a scan point) + exact free.
        let ts_lbl = if p.scan {
            format!(
                "{} {}",
                p.ts,
                if is_ja() {
                    "◆ スキャン時"
                } else {
                    "◆ scan"
                }
            )
        } else {
            p.ts.clone()
        };
        let free_lbl = if is_ja() {
            format!("空き {}", human(p.free))
        } else {
            format!("Free {}", human(p.free))
        };
        hits.push_str(&format!(
            "<circle cx='{px:.1}' cy='{py:.1}' r='7' fill='transparent' pointer-events='all' \
             class='ds-hit' data-ts='{ts}' data-free='{fl}'/>",
            ts = esc(&ts_lbl),
            fl = esc(&free_lbl),
        ));
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

    // Scan points get an emphasised orange diamond so "the moment you scanned"
    // stands out from the routine periodic samples.
    let mut scans = String::new();
    for (i, p) in pts.iter().enumerate() {
        if p.scan {
            let (sx, sy) = (x(i), y(p.free));
            scans.push_str(&format!(
                "<path d='M {sx:.1} {:.1} L {:.1} {sy:.1} L {sx:.1} {:.1} L {:.1} {sy:.1} Z' \
                 fill='#fb8500' stroke='#fff' stroke-width='1'/>",
                sy - 4.5,
                sx + 4.5,
                sy + 4.5,
                sx - 4.5,
            ));
        }
    }

    // Latest point marker.
    let (lx, ly) = (x(n - 1), y(pts[n - 1].free));

    // X labels: first and last timestamps (date + time is long, show as-is).
    let x_first = &first_ts(pts);
    let x_last = &pts[n - 1].ts;

    let svg = format!(
        "<svg id='ds-trend-svg' viewBox='0 0 {w:.0} {h:.0}' width='100%' style='max-width:{w:.0}px;height:auto;\
         border:1px solid #d0d7de;border-radius:10px;background:#fff'>\
         {grid}\
         <polyline points='{area}' fill='#0969da' fill-opacity='0.08' stroke='none'/>\
         <polyline points='{line}' fill='none' stroke='#0969da' stroke-width='2' \
         stroke-linejoin='round' stroke-linecap='round'/>\
         {scans}\
         <circle cx='{lx:.1}' cy='{ly:.1}' r='3.5' fill='#0969da'/>\
         <text x='{ml:.1}' y='{tb:.1}' font-size='11' fill='#57606a' text-anchor='start'>{xf}</text>\
         <text x='{xr:.1}' y='{tb:.1}' font-size='11' fill='#57606a' text-anchor='end'>{xl}</text>\
         {hits}\
         <g id='ds-tt' style='display:none' pointer-events='none'>\
         <rect id='ds-tt-bg' rx='6' fill='#1f2328' opacity='0.92'/>\
         <text id='ds-tt-tx' font-size='11' fill='#fff' text-anchor='middle'>\
         <tspan id='ds-tt-l1' x='0' y='0'></tspan><tspan id='ds-tt-l2' x='0' y='0'></tspan></text>\
         </g>\
         </svg>\
         <div style='color:#57606a;font-size:12px;margin-top:8px'>\
         <span style='color:#0969da'>—</span> {note} &nbsp; \
         <span style='color:#fb8500'>◆</span> {scan_legend}</div>",
        tb = h - 12.0,
        xr = ml + pw,
        xf = esc(x_first),
        xl = esc(x_last),
        note = if is_ja() {
            "空き容量（縦軸は増減が見えるよう自動ズーム・横軸は時間）"
        } else {
            "free space (Y auto-zoomed so changes show; X = time)"
        },
        scan_legend = if is_ja() {
            "スキャンした時点"
        } else {
            "a point recorded at scan time"
        },
    );
    format!("{svg}{TOOLTIP_JS}")
}

/// Hover behaviour for the trend chart. Each `.ds-hit` circle carries the exact
/// figures in `data-ts` / `data-free`; on hover we fill the in-SVG tooltip (so it
/// scales with the chart), size the background to the text via getBBox, and clamp
/// it inside the chart. Text is set with `textContent` only (never HTML), so the
/// server-escaped labels can't inject markup. No external script.
const TOOLTIP_JS: &str = r#"<script>(function(){
var svg=document.getElementById('ds-trend-svg');if(!svg)return;
var tt=document.getElementById('ds-tt'),bg=document.getElementById('ds-tt-bg'),
tx=document.getElementById('ds-tt-tx'),l1=document.getElementById('ds-tt-l1'),
l2=document.getElementById('ds-tt-l2');
var W=820,PAD=6,LH=14;
function show(c){
var x=parseFloat(c.getAttribute('cx')),y=parseFloat(c.getAttribute('cy'));
l1.textContent=c.getAttribute('data-ts')||'';
l2.textContent=c.getAttribute('data-free')||'';
var below=y<58;var top=below?y+12:y-34;
l1.setAttribute('x',x);l1.setAttribute('y',top);
l2.setAttribute('x',x);l2.setAttribute('y',top+LH);
tt.style.display='';
var b=tx.getBBox();var bw=b.width+PAD*2,bh=b.height+PAD*2;
var dx=0,left=x-bw/2,right=x+bw/2;
if(left<2)dx=2-left;else if(right>W-2)dx=(W-2)-right;
if(dx){l1.setAttribute('x',x+dx);l2.setAttribute('x',x+dx);b=tx.getBBox();}
bg.setAttribute('x',b.x-PAD);bg.setAttribute('y',b.y-PAD);
bg.setAttribute('width',bw);bg.setAttribute('height',bh);
}
function hide(){tt.style.display='none';}
svg.querySelectorAll('.ds-hit').forEach(function(c){
c.addEventListener('mouseenter',function(){show(c);});
c.addEventListener('mouseleave',hide);});
})();</script>"#;

fn first_ts(pts: &[Point]) -> String {
    pts.first().map(|p| p.ts.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_three_column_line_is_treated_as_a_scan_point() {
        // History written before the `kind` column existed was recorded at scan
        // time, so a missing 4th column must read back as a scan point.
        let p = parse_line("2026-11-21 08:00:00\t1000\t2000").unwrap();
        assert_eq!(p.free, 1000);
        assert!(p.scan);
    }

    #[test]
    fn kind_column_distinguishes_scan_from_auto() {
        assert!(parse_line("2026-11-21 08:00:00\t1\t2\tscan").unwrap().scan);
        assert!(!parse_line("2026-11-21 08:00:00\t1\t2\tauto").unwrap().scan);
    }

    #[test]
    fn malformed_lines_are_dropped() {
        assert!(parse_line("nope").is_none());
        assert!(parse_line("2026-11-21 08:00:00\tx\t2\tauto").is_none());
    }

    #[test]
    fn timestamp_parses_for_age_pruning() {
        assert!(parse_ts("2026-11-21 08:00:00").is_some());
        assert!(parse_ts("garbage").is_none());
    }

    #[test]
    fn downsample_caps_points_but_keeps_scans_and_last() {
        // A long auto history with a couple of scan points scattered in.
        let mut pts: Vec<Point> = (0..5000)
            .map(|i| Point {
                ts: format!("t{i}"),
                free: i as u64,
                scan: false,
            })
            .collect();
        pts[1234].scan = true;
        pts[4321].scan = true;
        let out = downsample(&pts);
        assert!(out.len() <= PLOT_CAP + 10, "plotted points stay capped");
        // Every scan point survives (so the ◆ markers never disappear)…
        assert!(out.iter().filter(|p| p.scan).count() == 2);
        // …and the most recent point is always kept.
        assert_eq!(out.last().unwrap().free, 4999);
    }

    #[test]
    fn downsample_is_noop_below_cap() {
        let pts: Vec<Point> = (0..10)
            .map(|i| Point {
                ts: format!("t{i}"),
                free: i as u64,
                scan: false,
            })
            .collect();
        assert_eq!(downsample(&pts).len(), 10);
    }
}
