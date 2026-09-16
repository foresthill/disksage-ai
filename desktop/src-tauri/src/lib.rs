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

/// Human-readable byte size (1024-based), matching the CLI/engine.
fn human(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut n = bytes as f64;
    let mut i = 0;
    while n >= 1024.0 && i < UNITS.len() - 1 {
        n /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{n:.1} {}", UNITS[i])
    }
}

/// Free space of the startup disk (mount "/"), else the largest disk. Read
/// in-process with sysinfo — no subprocess, no protected-folder access.
fn free_title() -> String {
    use sysinfo::Disks;
    let disks = Disks::new_with_refreshed_list();
    let free = disks
        .iter()
        .find(|d| d.mount_point() == Path::new("/"))
        .or_else(|| disks.iter().max_by_key(|d| d.total_space()))
        .map(|d| d.available_space())
        .unwrap_or(0);
    format!("💾 {}", human(free))
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
