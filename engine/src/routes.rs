//! Request router for the in-process HTTP UI.
//!
//! Split out of `serve.rs` so the (long) per-request dispatch lives apart from
//! the server bootstrap. `handle` matches (path, method) and delegates: POST
//! side-effect routes go to `actions`, GET pages are rendered here. The response
//! helpers below are shared with `actions` so every handler emits the same
//! headers / redirect shape as before — behaviour is unchanged by this split.

use tiny_http::{Header, Method, Request, Response};

use crate::actions;
use crate::ai;
use crate::breakdown;
use crate::lang::t;
use crate::reports;
use crate::settings;
use crate::state::State;
use crate::util::percent_decode;

/// HTML content-type header.
pub(crate) fn ctype() -> Header {
    Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).expect("hdr")
}

/// JSON content-type header.
pub(crate) fn jtype() -> Header {
    Header::from_bytes(
        &b"Content-Type"[..],
        &b"application/json; charset=utf-8"[..],
    )
    .expect("hdr")
}

/// 303 redirect to `location` — the POST-then-redirect pattern used throughout.
pub(crate) fn redirect(req: Request, location: &str) {
    let loc = Header::from_bytes(&b"Location"[..], location.as_bytes()).expect("hdr");
    let _ = req.respond(Response::empty(303).with_header(loc));
}

/// Respond with an HTML body.
pub(crate) fn html(req: Request, body: String) {
    let _ = req.respond(Response::from_string(body).with_header(ctype()));
}

/// Respond with a JSON body.
pub(crate) fn json(req: Request, body: String) {
    let _ = req.respond(Response::from_string(body).with_header(jtype()));
}

/// Respond with an empty body and a status code (204 / 303 / 403 / …).
pub(crate) fn empty(req: Request, code: u16) {
    let _ = req.respond(Response::empty(code));
}

/// Plain-text 404.
fn not_found(req: Request, msg: &str) {
    let _ = req.respond(Response::from_string(msg).with_status_code(404));
}

/// Dispatch one request. Takes the request by value; every arm responds to it.
pub(crate) fn handle(req: Request, state: &State) {
    let url = req.url().to_string();
    let path = url.split('?').next().unwrap_or("/").to_string();
    let post = *req.method() == Method::Post;
    match (path.as_str(), post) {
        ("/favicon.ico", _) => empty(req, 204),
        // POST side-effect routes (AI, chat, scans, delete, reveal, settings…).
        ("/ai", true) => actions::ai(req, state),
        ("/chat", true) => actions::chat(req, state),
        ("/rescan", true) => actions::rescan(req, state),
        ("/fullscan", true) => actions::fullscan(req, state),
        ("/drivescan", true) => actions::drivescan(req, state),
        ("/delete", true) => actions::delete(req, state),
        ("/reveal", true) => actions::reveal(req, state),
        ("/open-update", true) => actions::open_update(req, state),
        ("/open-logs", true) => actions::open_logs(req),
        ("/settings", true) => actions::settings_save(req, state),
        ("/breakdown", true) => actions::breakdown(req),
        // GET (or any-method) read routes.
        ("/report", _) => report_page(req, &url),
        ("/breakdown", _) => breakdown_page(req, &url),
        ("/ollama-models", _) => ollama_models(req, &url),
        ("/ai-status", _) => ai_status(req),
        ("/disk-usage", _) => disk_usage(req),
        ("/ai-preview", _) => ai_preview(req, state),
        // The main Scan page + the other shell pages (and the 404 fallback).
        _ => page(req, &url, &path, state),
    }
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
    // Wrap the bar in a stable container so the idle poller (see
    // `page::overview_poll_script`) can swap just this fragment in place.
    body.push_str("<div id='ds-overview'>");
    body.push_str(&crate::page::overview_html());
    body.push_str("</div>");
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
            } else {
                // Not scanning: offer a manual quick re-scan. This is the only
                // control that records a Trend point (Full scan doesn't), so it's
                // how you get the 2nd point to draw a line on /trend.
                body.push_str(&crate::page::rescan_button());
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
            body.push_str(&crate::drivescan_ui::panel_html(
                s.drive.as_ref(),
                s.drive_running,
            ));
            // Auto-refresh while a rescan, an AI call, the chat, Full or drive scan runs.
            (
                body,
                s.scanning || s.ai_running || s.deep_running || s.chat_running || s.drive_running,
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

/// A saved report opens inside the shell (keeps the sidebar + a back link),
/// not as a bare standalone page you can't navigate away from.
fn report_page(req: Request, url: &str) {
    match reports::query_param(url, "stamp").and_then(reports::saved_report_body) {
        Some(inner) => {
            let banner = t(
                "<div style='background:#fff8c5;border:1px solid #eac54f;border-radius:8px;\
                 padding:10px 14px;margin-bottom:14px;font-size:14px'>📁 Saved report (read-only) · \
                 <a href='/reports'>← All reports</a> · <a href='/'>Scan</a></div>",
                "<div style='background:#fff8c5;border:1px solid #eac54f;border-radius:8px;\
                 padding:10px 14px;margin-bottom:14px;font-size:14px'>📁 保存済みレポート（読み取り専用） · \
                 <a href='/reports'>← レポート一覧へ戻る</a> · <a href='/'>スキャン</a></div>",
            );
            let body = format!("{banner}{inner}");
            html(
                req,
                crate::page::shell("reports", t("Reports", "レポート"), &body, false),
            );
        }
        None => not_found(req, "report not found"),
    }
}

/// Aggregate drill-down (node_modules / Library caches): GET renders the
/// per-item picker. (The POST side lives in `actions::breakdown`.)
fn breakdown_page(req: Request, url: &str) {
    match reports::query_param(url, "kind").and_then(breakdown::Kind::parse) {
        Some(kind) => {
            let body = breakdown::page(kind);
            html(
                req,
                crate::page::shell("scan", t("Scan", "スキャン"), &body, false),
            );
        }
        None => not_found(req, "not found"),
    }
}

/// Live Ollama probe for the Settings "test & fetch models" button: GET the model
/// list from the URL the user is *typing* (query param), without saving first.
/// Returns JSON {ok, models|error}. Runs on the request thread (fine for a local,
/// single-user UI; capped by the client timeout).
fn ollama_models(req: Request, url: &str) {
    let typed = reports::query_param(url, "url")
        .map(percent_decode)
        .unwrap_or_default();
    let j = match crate::ollama::list_models_at(&typed) {
        Ok(models) => serde_json::json!({"ok": true, "models": models}),
        Err(e) => serde_json::json!({"ok": false, "error": e}),
    };
    json(req, j.to_string());
}

/// AI readiness lamp (🟢/🟡/🔴): the Scan page fetches this async so a slow Ollama
/// probe never blocks the page. Only Ollama is actually probed; BYOK / Claude Code
/// report "configured" (see ai_status).
fn ai_status(req: Request) {
    json(req, crate::ai_status::json());
}

/// Live disk-usage fragment: just the overview bar + snapshot note, so the Scan
/// page can refresh the free-space figure in place (after an empty-Trash /
/// delete) without reloading or re-running a scan.
fn disk_usage(req: Request) {
    html(req, crate::page::overview_html());
}

/// Transparency: show exactly the masked metadata the AI would receive.
fn ai_preview(req: Request, state: &State) {
    let findings = {
        let s = state.lock().unwrap();
        s.findings.clone()
    };
    let body = crate::ai_preview::page_html(findings.as_deref());
    html(
        req,
        crate::page::shell("scan", t("Scan", "スキャン"), &body, false),
    );
}

/// The Scan page and the remaining shell pages (/trend, /reports, /settings GET),
/// plus the 404 fallback for any unknown path.
fn page(req: Request, url: &str, path: &str, state: &State) {
    let out = match path {
        "/" | "/index.html" => {
            let deleted = reports::query_param(url, "deleted").is_some();
            let (mut body, refresh) = scan_page(state, deleted);
            // While a scan/AI/Full run is in flight the whole page already
            // meta-refreshes (updating the bar); only poll when idle.
            if !refresh {
                body.push_str(crate::page::overview_poll_script());
            }
            crate::page::shell("scan", t("Scan", "スキャン"), &body, refresh)
        }
        "/trend" => crate::page::shell(
            "trend",
            t("Trend", "推移"),
            &crate::trend::trend_html(),
            false,
        ),
        "/reports" => crate::page::shell(
            "reports",
            t("Reports", "レポート"),
            &reports::reports_html(),
            false,
        ),
        "/settings" => {
            let saved = reports::query_param(url, "saved").is_some();
            crate::page::shell(
                "settings",
                t("Settings", "設定"),
                &settings::settings_html(saved),
                false,
            )
        }
        _ => return not_found(req, "not found"),
    };
    html(req, out);
}
