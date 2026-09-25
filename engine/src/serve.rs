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

use crate::ai::{self, Analysis};
use crate::lang::{self, is_ja, t};
use crate::reports;
use crate::scan::{self, Finding};
use crate::settings;
use crate::trash;
use crate::util::percent_decode;

/// Shared scan state: findings are computed off the request path so `/` stays
/// responsive while the (slow) directory walk runs. AI judgments are opt-in and
/// also computed off the request path (a network call).
#[derive(Default)]
struct ScanState {
    scanning: bool,
    findings: Option<Vec<Finding>>,
    ai: Option<Result<Analysis, String>>,
    ai_running: bool,
    deep: Option<crate::deep::DeepScan>,
    deep_running: bool,
}
type State = Arc<Mutex<ScanState>>;

/// Kick off a scan in the background; `/` shows "scanning…" until it lands.
fn trigger_scan(state: State) {
    state.lock().unwrap().scanning = true;
    thread::spawn(move || {
        let found = scan::collect();
        // Save a snapshot so /reports has history even with no bash CLI present.
        if let Err(e) = crate::report::save(&found) {
            eprintln!("DiskSage: could not save report snapshot: {e}");
        }
        let mut s = state.lock().unwrap();
        s.findings = Some(found);
        s.ai = None; // previous judgments were for the old findings/indices
        s.deep = None; // deep results were for the previous scan
        s.scanning = false;
    });
}

/// The Scan page: overview (instant) + findings (or a scanning banner). Returns
/// the body and whether the page should auto-refresh (while a scan is running).
fn scan_page(state: &State) -> (String, bool) {
    let mut body = crate::page::overview_html();
    body.push_str(t(
        "<h2 style='font-size:16px;margin:22px 0 8px'>🎯 Findings</h2>",
        "<h2 style='font-size:16px;margin:22px 0 8px'>🎯 検出結果</h2>",
    ));
    let s = state.lock().unwrap();
    match &s.findings {
        Some(found) => {
            let ai_ref =
                s.ai.as_ref()
                    .and_then(|r| r.as_ref().ok())
                    .map(|a| a.judgments.as_slice());
            body.push_str(&crate::findings::findings_html(found, ai_ref));
            body.push_str(&crate::ai_ui::ai_controls(s.ai.as_ref(), s.ai_running));
            body.push_str(&crate::deep::section_html(s.deep.as_ref(), s.deep_running));
            // Auto-refresh while an AI call or the Full scan is in flight.
            (body, s.ai_running || s.deep_running)
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
pub fn run(port: u16) -> Result<(), String> {
    // Return an error on bind failure rather than exiting the process — the
    // desktop app runs this on a thread and must survive a port clash.
    let server = Server::http(("127.0.0.1", port))
        .map_err(|e| format!("cannot bind 127.0.0.1:{port}: {e}"))?;
    lang::init();
    let state: State = Arc::new(Mutex::new(ScanState::default()));
    trigger_scan(state.clone());
    eprintln!("DiskSage engine serving on http://127.0.0.1:{port}  (Ctrl-C to stop)");
    let ctype =
        || Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).expect("hdr");
    for mut req in server.incoming_requests() {
        let url = req.url().to_string();
        let path = url.split('?').next().unwrap_or("/");
        if path == "/favicon.ico" {
            let _ = req.respond(Response::empty(204));
            continue;
        }
        // AI judgment (opt-in): run analyze() in the background on the current
        // findings; the page shows "asking…" then the per-finding verdicts.
        if path == "/ai" && *req.method() == Method::Post {
            let to_analyze = {
                let s = state.lock().unwrap();
                if s.ai_running {
                    None
                } else {
                    s.findings.clone()
                }
            };
            if let Some(found) = to_analyze {
                {
                    let mut s = state.lock().unwrap();
                    s.ai_running = true;
                    s.ai = None;
                }
                let st = state.clone();
                thread::spawn(move || {
                    let result = ai::analyze(&found, is_ja());
                    let mut s = st.lock().unwrap();
                    s.ai = Some(result);
                    s.ai_running = false;
                });
            }
            let loc = Header::from_bytes(&b"Location"[..], &b"/"[..]).expect("hdr");
            let _ = req.respond(Response::empty(303).with_header(loc));
            continue;
        }
        // Full ("じっくり") scan (opt-in): biggest folders + recently-grown files,
        // computed off the request thread because it walks all of $HOME.
        if path == "/fullscan" && *req.method() == Method::Post {
            let go = {
                let mut s = state.lock().unwrap();
                if s.deep_running {
                    false
                } else {
                    s.deep_running = true;
                    s.deep = None;
                    true
                }
            };
            if go {
                let st = state.clone();
                thread::spawn(move || {
                    let result = crate::deep::run();
                    let mut s = st.lock().unwrap();
                    s.deep = Some(result);
                    s.deep_running = false;
                });
            }
            let loc = Header::from_bytes(&b"Location"[..], &b"/"[..]).expect("hdr");
            let _ = req.respond(Response::empty(303).with_header(loc));
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
                lang::refresh(); // apply the new language to the next request
            }
            let loc = Header::from_bytes(&b"Location"[..], &b"/settings?saved=1"[..]).expect("hdr");
            let _ = req.respond(Response::empty(303).with_header(loc));
            continue;
        }
        // A saved report opens inside the shell (keeps the sidebar + a back link),
        // not as a bare standalone page you can't navigate away from.
        if path == "/report" {
            match reports::query_param(&url, "stamp").and_then(reports::saved_report_body) {
                Some(inner) => {
                    let banner = t(
                        "<div style='background:#fff8c5;border:1px solid #eac54f;border-radius:8px;\
                         padding:10px 14px;margin-bottom:14px;font-size:14px'>📁 Saved report (read-only) · \
                         <a href='/'>← Back to Scan</a></div>",
                        "<div style='background:#fff8c5;border:1px solid #eac54f;border-radius:8px;\
                         padding:10px 14px;margin-bottom:14px;font-size:14px'>📁 保存済みレポート（読み取り専用） · \
                         <a href='/'>← スキャンに戻る</a></div>",
                    );
                    let body = format!("{banner}{inner}");
                    let html =
                        crate::page::shell("reports", t("Reports", "レポート"), &body, false);
                    let _ = req.respond(Response::from_string(html).with_header(ctype()));
                }
                None => {
                    let _ = req
                        .respond(Response::from_string("report not found").with_status_code(404));
                }
            }
            continue;
        }
        let html = match path {
            "/" | "/index.html" => {
                let (body, refresh) = scan_page(&state);
                crate::page::shell("scan", t("Scan", "スキャン"), &body, refresh)
            }
            "/reports" => crate::page::shell(
                "reports",
                t("Reports", "レポート"),
                &reports::reports_html(),
                false,
            ),
            "/settings" => {
                let saved = reports::query_param(&url, "saved").is_some();
                crate::page::shell(
                    "settings",
                    t("Settings", "設定"),
                    &settings::settings_html(saved),
                    false,
                )
            }
            _ => {
                let _ = req.respond(Response::from_string("not found").with_status_code(404));
                continue;
            }
        };
        let _ = req.respond(Response::from_string(html).with_header(ctype()));
    }
    Ok(())
}
