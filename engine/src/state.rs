//! Shared server state for the in-process HTTP UI (`serve`).
//!
//! Split out of `serve.rs` so the request router (`routes.rs`) and the POST
//! action handlers (`actions.rs`) can share the state type and the background
//! scan trigger without one giant file. Behaviour is unchanged — this is purely
//! where the `ScanState` struct and `trigger_scan` live now.

use std::sync::{Arc, Mutex};
use std::thread;

use crate::ai::Analysis;
use crate::scan::{self, Finding};

/// Shared scan state: findings are computed off the request path so `/` stays
/// responsive while the (slow) directory walk runs. AI judgments are opt-in and
/// also computed off the request path (a network call).
#[derive(Default)]
pub(crate) struct ScanState {
    pub(crate) scanning: bool,
    pub(crate) findings: Option<Vec<Finding>>,
    pub(crate) ai: Option<Result<Analysis, String>>,
    pub(crate) ai_running: bool,
    pub(crate) deep: Option<crate::deep::DeepScan>,
    pub(crate) deep_running: bool,
    /// Whole-drive MFT scan (Windows, admin) — the "where did it all go" view when
    /// the pattern scan finds nothing on a full drive. Ok result or a typed error.
    pub(crate) drive: Option<Result<crate::drivescan::DriveScan, crate::drivescan::DriveScanError>>,
    pub(crate) drive_running: bool,
    /// Follow-up conversation about the current findings (reset on a new scan).
    pub(crate) chat: Vec<crate::chat::ChatTurn>,
    pub(crate) chat_running: bool,
    /// A newer release found by the background update check (None = up to date,
    /// check disabled, offline, or app version unknown).
    pub(crate) update: Option<crate::update::Latest>,
}

pub(crate) type State = Arc<Mutex<ScanState>>;

/// Kick off a scan in the background; `/` shows "scanning…" until it lands.
pub(crate) fn trigger_scan(state: State) {
    state.lock().unwrap().scanning = true;
    thread::spawn(move || {
        let found = scan::collect();
        // Save a snapshot so /reports has history even with no bash CLI present.
        if let Err(e) = crate::report::save(&found) {
            eprintln!("DiskSage: could not save report snapshot: {e}");
        }
        // Record one free-space point for the /trend chart, marked as a scan
        // point (a special moment, emphasised on the chart). Best-effort.
        crate::trend::record(true);
        let mut s = state.lock().unwrap();
        s.findings = Some(found);
        s.ai = None; // previous judgments were for the old findings/indices
        s.deep = None; // deep results were for the previous scan
        s.drive = None; // whole-drive scan was for the previous state
        s.drive_running = false;
        s.chat.clear(); // the conversation was about the previous findings
        s.chat_running = false;
        s.scanning = false;
    });
}
