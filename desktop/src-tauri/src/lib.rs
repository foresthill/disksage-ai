// DiskSage desktop shell (approach A): a native window that displays the same
// local web UI served by `disksage serve`, plus a **menu bar tray** that always
// shows the startup disk's free space — the "ambient layer" idea, now first-party
// (no third-party app, no permission prompts). Disk stats are read in-process via
// `sysinfo`, so nothing is spawned and no directories/apps are touched.

use std::path::Path;
use std::process::{Child, Command};
use std::sync::Mutex;
use std::time::Duration;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;

/// Holds the spawned `disksage serve` child so we can terminate it on exit.
struct ServeProcess(Mutex<Option<Child>>);

const TRAY_ID: &str = "disksage-tray";
const REFRESH: Duration = Duration::from_secs(30);

/// Menu bar title: the startup disk's free space, computed in-process by the
/// DiskSage engine (no subprocess, no protected-folder access → no permission
/// dialogs). Same code path as the CLI, so the numbers always agree.
fn free_title() -> String {
    format!(
        "💾 {}",
        disksage_engine::util::human(disksage_engine::df::startup_free())
    )
}

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

/// Build the menu bar tray: free-space title + a small menu (Open / Quit).
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let open_i = MenuItem::with_id(app, "open", "Open DiskSage", true, None::<&str>)?;
    let quit_i = PredefinedMenuItem::quit(app, Some("Quit DiskSage"))?;
    let menu = Menu::with_items(app, &[&open_i, &quit_i])?;

    TrayIconBuilder::with_id(TRAY_ID)
        .title(free_title())
        .tooltip("DiskSage — startup disk free space")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "open" {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        })
        .build(app)?;

    // Refresh the title in the background so free space stays current.
    let handle = app.handle().clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(REFRESH);
        if let Some(tray) = handle.tray_by_id(TRAY_ID) {
            let _ = tray.set_title(Some(free_title()));
        }
    });
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(ServeProcess(Mutex::new(None)))
        .setup(|app| {
            if let Err(e) = setup_tray(app) {
                eprintln!("DiskSage: could not create the menu bar tray: {e}");
            }
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
