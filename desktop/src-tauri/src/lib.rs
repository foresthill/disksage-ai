// DiskSage desktop shell: a native window showing the DiskSage UI, plus a menu
// bar tray with the startup disk's free space.
//
// The UI is served **in-process** by the Rust engine (`disksage_engine::serve`),
// and the tray reads disk stats in-process too — so the app spawns nothing, needs
// no bash `disksage` on PATH, and raises no permission dialogs. This is what makes
// a future Windows/Linux build reachable: there is no macOS-only shell script in
// the app's path anymore.

use std::time::Duration;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;

const TRAY_ID: &str = "disksage-tray";
const REFRESH: Duration = Duration::from_secs(30);
/// Must match the port the window's index.html polls.
const SERVE_PORT: u16 = 8765;

/// Menu bar title: the startup disk's free space, computed in-process by the
/// DiskSage engine (no subprocess, no protected-folder access → no permission
/// dialogs). Same code path as the CLI, so the numbers always agree.
fn free_title() -> String {
    format!(
        "💾 {}",
        disksage_engine::util::human(disksage_engine::df::startup_free())
    )
}

/// Build the menu bar tray: free-space title + a small menu (Open / Quit).
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let open_i = MenuItem::with_id(app, "open", "Open DiskSage", true, None::<&str>)?;
    let quit_i = PredefinedMenuItem::quit(app, Some("Quit DiskSage"))?;
    let menu = Menu::with_items(app, &[&open_i, &quit_i])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
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
        });
    // An icon is required for the tray to be visible on Windows/Linux (where the
    // free-space *title* text isn't shown); on macOS the icon sits beside it.
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;

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
        .setup(|app| {
            if let Err(e) = setup_tray(app) {
                eprintln!("DiskSage: could not create the menu bar tray: {e}");
            }
            // Serve the UI in-process; the thread lives for the life of the app.
            std::thread::spawn(|| disksage_engine::serve::run(SERVE_PORT));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building the DiskSage application")
        .run(|_app, _event| {});
}
