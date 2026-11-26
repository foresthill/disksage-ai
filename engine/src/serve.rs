//! In-process HTTP server — porting the bash `serve` UI to Rust, so the desktop
//! app can eventually show its UI without spawning the macOS-only bash `serve`.
//!
//! This file is now just the bootstrap: bind the socket, kick off the background
//! workers (initial scan, update check, trend sampler) and hand each request to
//! `routes::handle`. The request router lives in `routes.rs`, the POST side-effect
//! handlers in `actions.rs`, and the shared state in `state.rs`.

use std::sync::{Arc, Mutex};
use std::thread;

use tiny_http::Server;

use crate::lang;
use crate::state::{trigger_scan, ScanState, State};

/// Background loop for the periodic trend sampler ("定点観測"). Wakes once a
/// minute and records a free-space point once `interval_minutes()` have elapsed
/// since the last one. Re-reading the config each tick means a Settings change
/// applies within a minute with no restart; when sampling is "off" it just waits.
/// The initial scan already records the first point, so the first periodic sample
/// lands one interval later (no duplicate at startup).
fn sampler_loop() {
    let tick = std::time::Duration::from_secs(60);
    let mut elapsed: u64 = 0; // minutes since the last recorded point
    loop {
        thread::sleep(tick);
        match crate::trend::interval_minutes() {
            // Disabled: reset so that re-enabling waits a full fresh interval.
            None => elapsed = 0,
            Some(interval) => {
                elapsed += 1;
                if elapsed >= interval {
                    crate::trend::record(false);
                    elapsed = 0;
                }
            }
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
    // Periodic free-space sampler ("定点観測"), off the request path: while the
    // app is running it appends a trend point every `trend_interval` minutes, so
    // the chart shows what happens between scans (e.g. space reclaimed overnight).
    // Config is re-read each tick, so changing the interval/retention in Settings
    // takes effect without a restart; "off" just skips recording.
    thread::spawn(sampler_loop);
    eprintln!("DiskSage engine serving on http://127.0.0.1:{port}  (Ctrl-C to stop)");
    for req in server.incoming_requests() {
        crate::routes::handle(req, &state);
    }
    Ok(())
}
