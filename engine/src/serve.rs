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
use crate::util::{esc, percent_decode};

/// Shared scan state: findings are computed off the request path so `/` stays
/// responsive while the (slow) directory walk runs. AI judgments are opt-in and
/// also computed off the request path (a network call).
#[derive(Default)]
struct ScanState {
    scanning: bool,
    findings: Option<Vec<Finding>>,
    ai: Option<Result<Analysis, String>>,
    ai_running: bool,
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


fn ai_configured() -> bool {
    ai::resolve_provider().is_ok()
}

fn ai_button(rerun: bool) -> String {
    let label = if rerun {
        t("🧠 Re-run AI judgment", "🧠 AI再判定")
    } else {
        t("🧠 Ask the AI to judge", "🧠 AIに判定してもらう")
    };
    let note = t(
        "Sends masked metadata only (no file contents) to your configured provider.",
        "設定済みプロバイダにマスク済みメタデータのみ送信（ファイル内容は送りません）。",
    );
    format!(
        "<form method='post' action='/ai' style='margin-top:12px'>\
         <button type='submit' style='background:#8250df;color:#fff;border:0;border-radius:8px;\
         padding:9px 18px;font-size:14px;cursor:pointer'>{label}</button>\
         <div style='color:#57606a;font-size:12px;margin-top:6px'>{note}</div></form>"
    )
}

/// The AI area under the findings: running banner, error, an opt-in button, or a
/// note when no BYOK key is configured.
fn ai_controls(s: &ScanState) -> String {
    if s.ai_running {
        return t(
            "<div style='margin-top:12px;padding:12px 14px;background:#ddf4ff;border:1px solid \
             #b6e3ff;border-radius:8px;font-size:14px'>🧠 Asking the AI…</div>",
            "<div style='margin-top:12px;padding:12px 14px;background:#ddf4ff;border:1px solid \
             #b6e3ff;border-radius:8px;font-size:14px'>🧠 AI が判定中…</div>",
        )
        .to_string();
    }
    if let Some(Err(e)) = &s.ai {
        return format!(
            "<div style='margin-top:12px;padding:10px 14px;background:#ffebe9;border:1px solid \
             #ff8182;border-radius:8px;font-size:13px'>⚠️ {} {}</div>{}",
            t("AI request failed:", "AI判定に失敗:"),
            esc(e),
            ai_button(true),
        );
    }
    // A completed analysis: show the token usage (factual, from the API) + re-run.
    if let Some(Ok(a)) = &s.ai {
        let usage = a
            .usage
            .as_ref()
            .map(|u| {
                format!(
                    "<div style='margin-top:12px;font-size:13px;color:#57606a'>🪙 {}: {} · {}: {}</div>",
                    t("input tokens", "入力トークン"),
                    u.input_tokens,
                    t("output tokens", "出力トークン"),
                    u.output_tokens,
                )
            })
            .unwrap_or_default();
        return format!("{usage}{}", ai_button(true));
    }
    if !ai_configured() {
        return format!(
            "<div style='margin-top:12px;color:#57606a;font-size:13px'>{}</div>",
            t(
                "AI judgment is available when a BYOK key is set (ANTHROPIC_API_KEY or OPENROUTER_API_KEY).",
                "AI判定は BYOK キー設定時に使えます（ANTHROPIC_API_KEY または OPENROUTER_API_KEY）。",
            )
        );
    }
    ai_button(s.ai.is_some())
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
            let ai_ref = s
                .ai
                .as_ref()
                .and_then(|r| r.as_ref().ok())
                .map(|a| a.judgments.as_slice());
            body.push_str(&crate::findings::findings_html(found, ai_ref));
            body.push_str(&ai_controls(&s));
            (body, s.ai_running) // auto-refresh while an AI call is in flight
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
                    let html = shell("reports", t("Reports", "レポート"), &body, false);
                    let _ = req.respond(Response::from_string(html).with_header(ctype()));
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
    Ok(())
}
