// DiskSage desktop shell (approach A): a native window that displays the same
// local web UI served by `disksage serve`. On startup we launch the CLI's
// server (with the browser auto-open suppressed, since we show it in-window),
// and we stop it again when the app quits.

use std::path::Path;
use std::process::{Child, Command};
use std::sync::Mutex;
use tauri::Manager;

/// Holds the spawned `disksage serve` child so we can terminate it on exit.
struct ServeProcess(Mutex<Option<Child>>);

/// Locate the `disksage` CLI. GUI-launched apps often have a minimal PATH, so
/// don't rely on it alone: honour DISKSAGE_BIN, then check common install
/// locations, then fall back to a bare PATH lookup.
fn resolve_disksage() -> String {
    if let Ok(p) = std::env::var("DISKSAGE_BIN") {
        if !p.is_empty() {
            return p;
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        "/usr/local/bin/disksage".to_string(),
        "/opt/homebrew/bin/disksage".to_string(),
        format!("{home}/.local/bin/disksage"),
        format!("{home}/bin/disksage"),
    ];
    for c in candidates {
        if Path::new(&c).exists() {
            return c;
        }
    }
    "disksage".to_string() // last resort: rely on PATH
}

fn start_serve() -> std::io::Result<Child> {
    // DISKSAGE_NO_BROWSER stops the CLI from opening a second, external browser.
    Command::new(resolve_disksage())
        .arg("serve")
        .env("DISKSAGE_NO_BROWSER", "1")
        .spawn()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(ServeProcess(Mutex::new(None)))
        .setup(|app| {
            match start_serve() {
                Ok(child) => {
                    *app.state::<ServeProcess>().0.lock().unwrap() = Some(child);
                }
                Err(e) => {
                    // The window's loading page will keep waiting; surface why.
                    eprintln!(
                        "DiskSage: could not start `disksage serve`: {e}. \
                         Is the `disksage` CLI on your PATH?"
                    );
                }
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building the DiskSage application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(mut child) = app.state::<ServeProcess>().0.lock().unwrap().take() {
                    let _ = child.kill();
                }
            }
        });
}
