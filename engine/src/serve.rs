//! In-process HTTP server — increment 1 of porting the bash `serve` UI to Rust.
//!
//! Serves the disk-usage overview from the engine's own `df` data, so the
//! desktop app can eventually show its UI without spawning the macOS-only bash
//! `serve`. English-only and overview-only for now; findings, reports, settings,
//! the delete-to-Trash flow and i18n come in later increments.

use tiny_http::{Header, Response, Server};

use crate::df;
use crate::util::human;

/// Left navigation, matching the bash serve's sidebar.
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
        item("/", "🔍", "Scan", "scan"),
        item("/reports", "📁", "Reports", "reports"),
        item("/settings", "⚙️", "Settings", "settings"),
    )
}

/// Full HTML document: the sidebar plus a body, content shifted right to clear it.
fn shell(active: &str, title: &str, body: &str) -> String {
    format!(
        "<!doctype html><html lang='en'><head><meta charset='utf-8'>\
         <meta name='viewport' content='width=device-width, initial-scale=1'>\
         <title>DiskSage — {title}</title>\
         <style>body{{padding-left:200px}}@media(max-width:640px){{body{{padding-left:0}}}}</style></head>\
         <body style='font-family:-apple-system,BlinkMacSystemFont,Helvetica,Arial,sans-serif;\
         margin:0;background:#f6f8fa;color:#1f2328'>{}\
         <div style='max-width:880px;margin:24px auto;background:#fff;border:1px solid #d0d7de;\
         border-radius:12px;padding:28px 32px'>{body}</div></body></html>",
        sidebar(active)
    )
}

/// The disk-usage overview (one bar per APFS container) + snapshot count.
fn overview_body() -> String {
    let mut parts = String::from("<h2 style='font-size:16px;margin:6px 0 14px'>📊 Current Disk Usage</h2>");
    for (total, members) in df::containers().iter().rev() {
        let free = members.iter().map(|m| m.avail).max().unwrap_or(0);
        let used = total.saturating_sub(free);
        let pct = if *total > 0 { used * 100 / total } else { 0 };
        let color = if pct >= 90 { "#d1242f" } else if pct >= 75 { "#bf8700" } else { "#1a7f37" };
        parts.push_str(&format!(
            "<div style='margin:10px 0'><div style='font-weight:600'>{}</div>\
             <div style='display:flex;align-items:center;gap:10px'>\
             <span style='flex:1;height:12px;background:#e6e6e6;border-radius:6px;overflow:hidden'>\
             <span style='display:block;height:100%;width:{}%;background:{color}'></span></span>\
             <span style='white-space:nowrap;color:#57606a;font-size:13px'>{} / {} ({}%) · free {}</span>\
             </div></div>",
            df::label_for(members),
            pct.min(100),
            human(used),
            human(*total),
            pct,
            human(free),
        ));
    }
    match df::snapshot_count() {
        Some(0) | None => {}
        Some(n) => parts.push_str(&format!(
            "<div style='margin-top:16px;padding:12px 14px;background:#fff8c5;border:1px solid #eac54f;\
             border-radius:8px;font-size:13px'>Local snapshots: {n} — these can hold space \
             (each keeps recently-deleted files alive).</div>"
        )),
    }
    parts.push_str(
        "<p style='color:#57606a;font-size:12px;margin-top:18px'>Findings, reports, settings and \
         the delete flow are being ported to this Rust server; for now they live in the CLI.</p>",
    );
    parts
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
    eprintln!("DiskSage engine serving on http://127.0.0.1:{port}  (Ctrl-C to stop)");
    for req in server.incoming_requests() {
        let path = req.url().split('?').next().unwrap_or("/").to_string();
        if path == "/favicon.ico" {
            let _ = req.respond(Response::empty(204));
            continue;
        }
        let html = match path.as_str() {
            "/" | "/index.html" => shell("scan", "Disk Usage", &overview_body()),
            "/reports" => shell("reports", "Reports", "<p>Coming soon in the Rust server.</p>"),
            "/settings" => shell("settings", "Settings", "<p>Coming soon in the Rust server.</p>"),
            _ => {
                let _ = req.respond(Response::from_string("not found").with_status_code(404));
                continue;
            }
        };
        let ctype = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
            .expect("valid header");
        let _ = req.respond(Response::from_string(html).with_header(ctype));
    }
}
