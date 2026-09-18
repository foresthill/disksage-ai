//! In-process HTTP server — porting the bash `serve` UI to Rust, so the desktop
//! app can eventually show its UI without spawning the macOS-only bash `serve`.
//!
//! Increment 1: disk-usage overview.
//! Increment 2 (this file): findings, scanned in the background so the overview
//! paints instantly and the findings stream in (progressive, like bash serve).
//! Still English-only; /reports, /settings, delete-to-Trash and i18n come next.

use std::sync::{Arc, Mutex};
use std::thread;

use tiny_http::{Header, Method, Response, Server};

use crate::df;
use crate::lang::{self, is_ja, t};
use crate::reports;
use crate::scan::{self, Finding};
use crate::settings;
use crate::trash;
use crate::util::{esc, human, percent_decode};

/// Shared scan state: findings are computed off the request path so `/` stays
/// responsive while the (slow) directory walk runs.
#[derive(Default)]
struct ScanState {
    scanning: bool,
    findings: Option<Vec<Finding>>,
}
type State = Arc<Mutex<ScanState>>;

/// Kick off a scan in the background; `/` shows "scanning…" until it lands.
fn trigger_scan(state: State) {
    state.lock().unwrap().scanning = true;
    thread::spawn(move || {
        let found = scan::collect();
        let mut s = state.lock().unwrap();
        s.findings = Some(found);
        s.scanning = false;
    });
}

fn sidebar(active: &str) -> String {
    let item = |href: &str, icon: &str, label: &str, key: &str| {
        let bg = if key == active { "background:#30363d;" } else { "" };
        format!(
            "<a href='{href}' style='display:block;padding:10px 12px;border-radius:8px;\
             color:#fff;text-decoration:none;margin:2px 0;font-size:14px;{bg}'>{icon} {label}</a>"
        )
    };
    format!(
        "<nav style='position:fixed;left:0;top:0;bottom:0;width:200px;background:#1f2328;\
         color:#fff;padding:18px 14px;box-sizing:border-box;z-index:20;overflow:auto'>\
         <div style='font-weight:700;font-size:17px;margin:2px 0 18px'>🩺 DiskSage</div>{}{}{}</nav>",
        item("/", "🔍", t("Scan", "スキャン"), "scan"),
        item("/reports", "📁", t("Reports", "レポート"), "reports"),
        item("/settings", "⚙️", t("Settings", "設定"), "settings"),
    )
}

fn shell(active: &str, title: &str, body: &str, refresh: bool) -> String {
    let meta = if refresh {
        "<meta http-equiv='refresh' content='2'>"
    } else {
        ""
    };
    format!(
        "<!doctype html><html lang='{lang}'><head><meta charset='utf-8'>{meta}\
         <meta name='viewport' content='width=device-width, initial-scale=1'>\
         <title>DiskSage — {title}</title>\
         <style>body{{padding-left:200px}}@media(max-width:640px){{body{{padding-left:0}}}}</style></head>\
         <body style='font-family:-apple-system,BlinkMacSystemFont,Helvetica,Arial,sans-serif;\
         margin:0;background:#f6f8fa;color:#1f2328'>{sidebar}\
         <div style='max-width:880px;margin:24px auto;background:#fff;border:1px solid #d0d7de;\
         border-radius:12px;padding:28px 32px'>{body}</div></body></html>",
        lang = t("en", "ja"),
        sidebar = sidebar(active),
    )
}

/// The disk-usage overview (one bar per APFS container) + snapshot warning.
fn overview_html() -> String {
    let heading = t(
        "<h2 style='font-size:16px;margin:6px 0 14px'>📊 Current Disk Usage</h2>",
        "<h2 style='font-size:16px;margin:6px 0 14px'>📊 現在のディスク使用量</h2>",
    );
    let mut parts = String::from(heading);
    for (total, members) in df::containers().iter().rev() {
        let free = members.iter().map(|m| m.avail).max().unwrap_or(0);
        let used = total.saturating_sub(free);
        let pct = if *total > 0 { used * 100 / total } else { 0 };
        let color = if pct >= 90 { "#d1242f" } else if pct >= 75 { "#bf8700" } else { "#1a7f37" };
        let raw = df::label_for(members);
        let label = if raw == "Startup disk" { t("Startup disk", "起動ディスク") } else { raw };
        parts.push_str(&format!(
            "<div style='margin:10px 0'><div style='font-weight:600'>{}</div>\
             <div style='display:flex;align-items:center;gap:10px'>\
             <span style='flex:1;height:12px;background:#e6e6e6;border-radius:6px;overflow:hidden'>\
             <span style='display:block;height:100%;width:{}%;background:{color}'></span></span>\
             <span style='white-space:nowrap;color:#57606a;font-size:13px'>{} / {} ({}%) · {} {}</span>\
             </div></div>",
            esc(label),
            pct.min(100),
            human(used),
            human(*total),
            pct,
            t("free", "空き"),
            human(free),
        ));
    }
    if let Some(n) = df::snapshot_count() {
        if n > 0 {
            let msg = if is_ja() {
                format!(
                    "ローカルスナップショット: {n} 個 — 空きを保持している可能性（削除済みファイルを生かし続けます）。"
                )
            } else {
                format!(
                    "Local snapshots: {n} — these can hold space (each keeps recently-deleted files alive)."
                )
            };
            parts.push_str(&format!(
                "<div style='margin-top:14px;padding:12px 14px;background:#fff8c5;border:1px solid #eac54f;\
                 border-radius:8px;font-size:13px'>{msg}</div>"
            ));
        }
    }
    parts
}

/// The Scan page: overview (instant) + findings (or a scanning banner). Returns
/// the body and whether the page should auto-refresh (while a scan is running).
fn scan_page(state: &State) -> (String, bool) {
    let mut body = overview_html();
    body.push_str(t(
        "<h2 style='font-size:16px;margin:22px 0 8px'>🎯 Findings</h2>",
        "<h2 style='font-size:16px;margin:22px 0 8px'>🎯 検出結果</h2>",
    ));
    let s = state.lock().unwrap();
    match &s.findings {
        Some(found) => {
            body.push_str(&crate::findings::findings_html(found));
            (body, false)
        }
        None => {
            body.push_str(t(
                "<div style='padding:14px 16px;background:#ddf4ff;border:1px solid #b6e3ff;\
                 border-radius:8px;font-size:14px'>🔍 Scanning… findings will appear here.</div>",
                "<div style='padding:14px 16px;background:#ddf4ff;border:1px solid #b6e3ff;\
                 border-radius:8px;font-size:14px'>🔍 スキャン中… 検出結果がここに表示されます。</div>",
            ));
            (body, true)
        }
    }
}

/// Serve on 127.0.0.1:port until interrupted. Sequential — fine for a local,
/// single-user UI.
pub fn run(port: u16) {
    let server = match Server::http(("127.0.0.1", port)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("serve: cannot bind 127.0.0.1:{port}: {e}");
            std::process::exit(1);
        }
    };
    lang::init();
    let state: State = Arc::new(Mutex::new(ScanState::default()));
    trigger_scan(state.clone());
    eprintln!("DiskSage engine serving on http://127.0.0.1:{port}  (Ctrl-C to stop)");
    let ctype = || {
        Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).expect("hdr")
    };
    for mut req in server.incoming_requests() {
        let url = req.url().to_string();
        let path = url.split('?').next().unwrap_or("/");
        if path == "/favicon.ico" {
            let _ = req.respond(Response::empty(204));
            continue;
        }
        // Deleting: move whitelisted, currently-offered paths to the Trash.
        if path == "/delete" && *req.method() == Method::Post {
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);
            let requested: Vec<String> = body
                .split('&')
                .filter_map(|kv| {
                    let (k, v) = kv.split_once('=')?;
                    (k == "del").then(|| percent_decode(v))
                })
                .collect();
            // Validate every requested path against the paths the server is
            // *currently* offering as deletable — never trust the client.
            let allowed: std::collections::HashSet<String> = {
                let s = state.lock().unwrap();
                s.findings
                    .as_ref()
                    .map(|fs| {
                        fs.iter()
                            .filter(|f| trash::is_deletable(f.id))
                            .map(|f| f.path.clone())
                            .collect()
                    })
                    .unwrap_or_default()
            };
            for p in &requested {
                if allowed.contains(p) {
                    let _ = trash::to_trash(&trash::expand_tilde(p));
                }
            }
            trigger_scan(state.clone()); // refresh findings after deletion
            let loc = Header::from_bytes(&b"Location"[..], &b"/"[..]).expect("hdr");
            let _ = req.respond(Response::empty(303).with_header(loc));
            continue;
        }
        // Saving settings: write the language choice to the shared config file.
        if path == "/settings" && *req.method() == Method::Post {
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);
            let val = settings::form_value(&body, "lang").unwrap_or("");
            if matches!(val, "" | "ja" | "en") {
                settings::write_lang(val);
            }
            let loc = Header::from_bytes(&b"Location"[..], &b"/settings?saved=1"[..]).expect("hdr");
            let _ = req.respond(Response::empty(303).with_header(loc));
            continue;
        }
        // A saved report opens as its own (bash-generated) HTML page.
        if path == "/report" {
            match reports::query_param(&url, "stamp").and_then(reports::saved_report) {
                Some(doc) => {
                    let _ = req.respond(Response::from_string(doc).with_header(ctype()));
                }
                None => {
                    let _ = req.respond(Response::from_string("report not found").with_status_code(404));
                }
            }
            continue;
        }
        let html = match path {
            "/" | "/index.html" => {
                let (body, refresh) = scan_page(&state);
                shell("scan", t("Scan", "スキャン"), &body, refresh)
            }
            "/reports" => shell("reports", t("Reports", "レポート"), &reports::reports_html(), false),
            "/settings" => {
                let saved = reports::query_param(&url, "saved").is_some();
                shell("settings", t("Settings", "設定"), &settings::settings_html(saved), false)
            }
            _ => {
                let _ = req.respond(Response::from_string("not found").with_status_code(404));
                continue;
            }
        };
        let _ = req.respond(Response::from_string(html).with_header(ctype()));
    }
}
