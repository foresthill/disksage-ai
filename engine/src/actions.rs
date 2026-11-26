//! POST side-effect handlers for the in-process HTTP UI.
//!
//! Split out of `serve.rs`: each `pub(crate)` fn here is one route's handler,
//! called from `routes::handle`. They share the response helpers in `routes`
//! (`redirect`, `empty`) so the 303/204/403 shapes are identical to before.
//! Every handler that kicks off slow work (AI, chat, scans) still does so on a
//! background thread under the same state-locking discipline.

use std::collections::HashSet;
use std::thread;

use tiny_http::Request;

use crate::ai;
use crate::lang::is_ja;
use crate::routes::{empty, redirect};
use crate::settings;
use crate::state::{trigger_scan, State};
use crate::trash;
use crate::util::percent_decode;

/// AI judgment (opt-in): run analyze() in the background on the current findings;
/// the page shows "asking…" then the per-finding verdicts.
pub(crate) fn ai(req: Request, state: &State) {
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
    redirect(req, "/");
}

/// Follow-up chat (opt-in): append the user's message, then answer it off the
/// request thread (a network / CLI call). The findings context is masked.
pub(crate) fn chat(mut req: Request, state: &State) {
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
            let reply = crate::chat::reply(&history, &found, ja, crate::audit::enabled(false));
            let content = crate::chat::format_reply(reply, ja);
            let mut s = st.lock().unwrap();
            s.chat.push(crate::chat::ChatTurn {
                role: "assistant".into(),
                content,
            });
            s.chat_running = false;
        });
    }
    redirect(req, "/");
}

/// Manual quick re-scan: re-run the quick scan (which records a /trend point via
/// trigger_scan → trend::record), then redirect to /. Guarded so a double-submit
/// doesn't stack scans. Same shape as /fullscan.
pub(crate) fn rescan(req: Request, state: &State) {
    let go = {
        let s = state.lock().unwrap();
        !s.scanning
    };
    if go {
        trigger_scan(state.clone());
    }
    redirect(req, "/");
}

/// Full ("じっくり") scan (opt-in): biggest folders + recently-grown files,
/// computed off the request thread because it walks all of $HOME.
pub(crate) fn fullscan(req: Request, state: &State) {
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
    redirect(req, "/");
}

/// Whole-drive MFT scan (Windows, admin). Runs off the request thread because it
/// reads the entire volume. Same guard shape as /fullscan.
pub(crate) fn drivescan(req: Request, state: &State) {
    let go = {
        let mut s = state.lock().unwrap();
        if s.drive_running {
            false
        } else {
            s.drive_running = true;
            s.drive = None;
            true
        }
    };
    if go {
        let st = state.clone();
        thread::spawn(move || {
            let result = crate::drivescan::scan_system_drive(25);
            let mut s = st.lock().unwrap();
            s.drive = Some(result);
            s.drive_running = false;
        });
    }
    redirect(req, "/");
}

/// Deleting: move whitelisted, currently-offered paths to the Trash.
pub(crate) fn delete(mut req: Request, state: &State) {
    let mut body = String::new();
    let _ = req.as_reader().read_to_string(&mut body);
    let requested: Vec<String> = body
        .split('&')
        .filter_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            (k == "del").then(|| percent_decode(v))
        })
        .collect();
    // Validate every requested path against the paths the server is *currently*
    // offering as deletable — never trust the client.
    let allowed: HashSet<String> = {
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
    redirect(req, "/?deleted=1");
}

/// Reveal a finding's path in the file manager. Read-only (opens a window), but
/// still validated: only a path the server is *currently* showing as a finding is
/// honoured, never an arbitrary path from the client.
pub(crate) fn reveal(mut req: Request, state: &State) {
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
    empty(req, code);
}

/// Open the new-version release page in the default browser. The URL is the one
/// the engine's own update check produced (never a client-supplied one), so a
/// cross-origin Tauri webview — where a plain target=_blank link does nothing —
/// still reaches the download page.
pub(crate) fn open_update(req: Request, state: &State) {
    let url = state.lock().unwrap().update.as_ref().map(|u| u.url.clone());
    if let Some(u) = url {
        let _ = crate::reveal::open_url(&u);
    }
    empty(req, 204);
}

/// Open the AI audit-log folder in the file manager. Fixed, app-owned path
/// (audit::logs_dir()); created if missing so the window isn't empty-404.
pub(crate) fn open_logs(req: Request) {
    let dir = crate::audit::logs_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = crate::reveal::open_folder(&dir);
    empty(req, 204);
}

/// Saving settings: language (→ config) or the AI key (→ OS keychain).
pub(crate) fn settings_save(mut req: Request, state: &State) {
    let mut body = String::new();
    let _ = req.as_reader().read_to_string(&mut body);
    // A language change regenerates findings: their text is baked at scan time,
    // so the current results stay in the old language until a rescan.
    if settings::handle_post(&body) {
        trigger_scan(state.clone());
    }
    redirect(req, "/settings?saved=1");
}

/// Aggregate drill-down POST: trash the selected sub-items (validated
/// structurally inside breakdown::handle_post, never trusting the posted path).
pub(crate) fn breakdown(mut req: Request) {
    let mut body = String::new();
    let _ = req.as_reader().read_to_string(&mut body);
    let loc = crate::breakdown::handle_post(&body); // parse + validate + trash
    redirect(req, &loc);
}

/// Save / clear a per-finding note. Plain data keyed by the path string: notes
/// never touch the filesystem (so, unlike /delete and /reveal, no whitelist is
/// needed) and are not sent to the AI here — storage only.
pub(crate) fn note(mut req: Request) {
    let mut body = String::new();
    let _ = req.as_reader().read_to_string(&mut body);
    let loc = crate::notes::handle_post(&body);
    redirect(req, &loc);
}
