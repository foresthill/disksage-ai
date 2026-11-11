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
use crate::breakdown;
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
    /// Follow-up conversation about the current findings (reset on a new scan).
    chat: Vec<crate::chat::ChatTurn>,
    chat_running: bool,
    /// A newer release found by the background update check (None = up to date,
    /// check disabled, offline, or app version unknown).
    update: Option<crate::update::Latest>,
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
        s.chat.clear(); // the conversation was about the previous findings
        s.chat_running = false;
        s.scanning = false;
    });
}

/// The Scan page: overview (instant) + findings (or a scanning banner). Returns
/// the body and whether the page should auto-refresh (while a scan is running).
fn scan_page(state: &State, deleted: bool) -> (String, bool) {
    let mut body = String::new();
    // Prepend the "new version" banner if the background check found one. Taken
    // in its own lock scope so we don't hold the lock across the page build.
    if let Some(u) = &state.lock().unwrap().update {
        body.push_str(&crate::page::update_banner(&u.tag, &u.url));
    }
    // After a delete: the files went to the Trash, so the disk bar below won't
    // drop until the Trash is emptied. Say so, right where the confusion happens.
    if deleted {
        body.push_str(&crate::page::trash_banner());
    }
    body.push_str(&crate::page::overview_html());
    body.push_str(t(
        "<h2 style='font-size:16px;margin:22px 0 8px'>🎯 Findings</h2>",
        "<h2 style='font-size:16px;margin:22px 0 8px'>🎯 検出結果</h2>",
    ));
    let s = state.lock().unwrap();
    match &s.findings {
        Some(found) => {
            // A rescan is running (e.g. just after a delete): the list below is
            // the previous result; show a note and auto-refresh until it lands.
            if s.scanning {
                body.push_str(t(
                    "<div style='margin:8px 0;padding:10px 12px;background:#fff8c5;border:1px solid \
                     #eac54f;border-radius:8px;font-size:13px'>🔄 Re-scanning… the list updates automatically.</div>",
                    "<div style='margin:8px 0;padding:10px 12px;background:#fff8c5;border:1px solid \
                     #eac54f;border-radius:8px;font-size:13px'>🔄 再スキャン中… 結果は自動で更新されます。</div>",
                ));
            }
            let ai_ref =
                s.ai.as_ref()
                    .and_then(|r| r.as_ref().ok())
                    .map(|a| a.judgments.as_slice());
            body.push_str(&crate::findings::findings_html(found, ai_ref));
            body.push_str(&crate::ai_ui::ai_controls(s.ai.as_ref(), s.ai_running));
            // Follow-up chat about the findings — once an AI backend is available.
            if ai::available() {
                body.push_str(&crate::chat_ui::panel_html(&s.chat, s.chat_running));
            }
            body.push_str(&crate::deep::section_html(s.deep.as_ref(), s.deep_running));
            // Auto-refresh while a rescan, an AI call, the chat, or Full scan runs.
            (
                body,
                s.scanning || s.ai_running || s.deep_running || s.chat_running,
            )
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
    // Signature-free update check, off the request path: if a newer release
    // exists, the Scan page shows a link. Best-effort and opt-out (see update.rs).
    {
        let st = state.clone();
        thread::spawn(move || {
            if let Some(latest) = crate::update::check() {
                st.lock().unwrap().update = Some(latest);
            }
        });
    }
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
        // Follow-up chat (opt-in): append the user's message, then answer it off the
        // request thread (a network / CLI call). The findings context is masked.
        if path == "/chat" && *req.method() == Method::Post {
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);
            let msg = body
                .split('&')
                .find_map(|kv| {
                    let (k, v) = kv.split_once('=')?;
                    (k == "msg").then(|| percent_decode(v))
                })
                .unwrap_or_default();
            let msg = msg.trim().to_string();
            let launch = {
                let mut s = state.lock().unwrap();
                if msg.is_empty() || s.chat_running || s.findings.is_none() {
                    None
                } else {
                    s.chat.push(crate::chat::ChatTurn {
                        role: "user".into(),
                        content: msg,
                    });
                    s.chat_running = true;
                    Some((s.chat.clone(), s.findings.clone().unwrap()))
                }
            };
            if let Some((history, found)) = launch {
                let st = state.clone();
                let ja = is_ja();
                thread::spawn(move || {
                    let reply =
                        crate::chat::reply(&history, &found, ja, crate::audit::enabled(false));
                    let content = crate::chat::format_reply(reply, ja);
                    let mut s = st.lock().unwrap();
                    s.chat.push(crate::chat::ChatTurn {
                        role: "assistant".into(),
                        content,
                    });
                    s.chat_running = false;
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
            let loc = Header::from_bytes(&b"Location"[..], &b"/?deleted=1"[..]).expect("hdr");
            let _ = req.respond(Response::empty(303).with_header(loc));
            continue;
        }
        // Reveal a finding's path in the file manager. Read-only (opens a window),
        // but still validated: only a path the server is *currently* showing as a
        // finding is honoured, never an arbitrary path from the client.
        if path == "/reveal" && *req.method() == Method::Post {
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);
            let requested = body.split('&').find_map(|kv| {
                let (k, v) = kv.split_once('=')?;
                (k == "path").then(|| percent_decode(v))
            });
            let known: bool = requested
                .as_ref()
                .map(|p| {
                    let s = state.lock().unwrap();
                    s.findings
                        .as_ref()
                        .map(|fs| fs.iter().any(|f| &f.path == p))
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            let code = if known {
                let _ = crate::reveal::reveal(&trash::expand_tilde(&requested.unwrap()));
                204
            } else {
                403
            };
            let _ = req.respond(Response::empty(code));
            continue;
        }
        // Open the AI audit-log folder in the file manager. Fixed, app-owned path
        // (audit::logs_dir()); created if missing so the window isn't empty-404.
        if path == "/open-logs" && *req.method() == Method::Post {
            let dir = crate::audit::logs_dir();
            let _ = std::fs::create_dir_all(&dir);
            let _ = crate::reveal::open_folder(&dir);
            let _ = req.respond(Response::empty(204));
            continue;
        }
        // Saving settings: language (→ config) or the AI key (→ OS keychain).
        if path == "/settings" && *req.method() == Method::Post {
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);
            settings::handle_post(&body);
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
        // Aggregate drill-down (node_modules / Library caches): GET renders the
        // per-item picker; POST trashes the selected sub-items (validated
        // structurally, never trusting the posted path).
        if path == "/breakdown" {
            if *req.method() == Method::Post {
                let mut body = String::new();
                let _ = req.as_reader().read_to_string(&mut body);
                let loc = breakdown::handle_post(&body); // parse + validate + trash
                let hdr = Header::from_bytes(&b"Location"[..], loc.as_bytes()).expect("hdr");
                let _ = req.respond(Response::empty(303).with_header(hdr));
                continue;
            }
            match reports::query_param(&url, "kind").and_then(breakdown::Kind::parse) {
                Some(kind) => {
                    let html = crate::page::shell(
                        "scan",
                        t("Scan", "スキャン"),
                        &breakdown::page(kind),
                        false,
                    );
                    let _ = req.respond(Response::from_string(html).with_header(ctype()));
                }
                None => {
                    let _ = req.respond(Response::from_string("not found").with_status_code(404));
                }
            }
            continue;
        }
        // Live Ollama probe for the Settings "test & fetch models" button: GET the
        // model list from the URL the user is *typing* (query param), without
        // saving first. Returns JSON {ok, models|error}. Runs on the request
        // thread (fine for a local, single-user UI; capped by the client timeout).
        if path == "/ollama-models" {
            let typed = reports::query_param(&url, "url")
                .map(percent_decode)
                .unwrap_or_default();
            let json = match crate::ollama::list_models_at(&typed) {
                Ok(models) => serde_json::json!({"ok": true, "models": models}),
                Err(e) => serde_json::json!({"ok": false, "error": e}),
            };
            let jtype = Header::from_bytes(
                &b"Content-Type"[..],
                &b"application/json; charset=utf-8"[..],
            )
            .expect("hdr");
            let _ = req.respond(Response::from_string(json.to_string()).with_header(jtype));
            continue;
        }
        // Transparency: show exactly the masked metadata the AI would receive.
        if path == "/ai-preview" {
            let findings = {
                let s = state.lock().unwrap();
                s.findings.clone()
            };
            let body = crate::ai_preview::page_html(findings.as_deref());
            let html = crate::page::shell("scan", t("Scan", "スキャン"), &body, false);
            let _ = req.respond(Response::from_string(html).with_header(ctype()));
            continue;
        }
        let html = match path {
            "/" | "/index.html" => {
                let deleted = reports::query_param(&url, "deleted").is_some();
                let (body, refresh) = scan_page(&state, deleted);
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
