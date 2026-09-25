// DiskSage desktop shell: a native window showing the DiskSage UI, plus a menu
// bar tray with the startup disk's free space.
//
// The UI is served **in-process** by the Rust engine (`disksage_engine::serve`),
// and the tray reads disk stats in-process too — so the app spawns nothing, needs
// no bash `disksage` on PATH, and raises no permission dialogs. This is what makes
// a future Windows/Linux build reachable: there is no macOS-only shell script in
// the app's path anymore.
//
// Menu-bar-resident behaviour (so the tray doesn't vanish the moment you close
// the window, and can come back after a reboot):
//   - Closing the window HIDES it instead of quitting — the app lives in the tray.
//   - macOS: no Dock icon (Accessory activation policy) → menu-bar-only.
//   - An opt-in "Start at Login" tray toggle registers/unregisters a login item.

use std::time::Duration;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

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

/// Show and focus the main window (used from the tray "Open" item).
fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Build the menu bar tray: free-space title + a small menu
/// (Open / Start at Login / Quit).
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let open_i = MenuItem::with_id(app, "open", "Open DiskSage", true, None::<&str>)?;
    // Reflect the current login-item state so the checkmark is truthful.
    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);
    let login_i = CheckMenuItem::with_id(
        app,
        "start_at_login",
        "Start at Login",
        true,
        autostart_on,
        None::<&str>,
    )?;
    let quit_i = PredefinedMenuItem::quit(app, Some("Quit DiskSage"))?;
    let menu = Menu::with_items(app, &[&open_i, &login_i, &quit_i])?;

    // The toggle updates its own checkmark only when the (un)register succeeds.
    let login_for_cb = login_i.clone();
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .title(free_title())
        .tooltip("DiskSage — startup disk free space")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "start_at_login" => {
                let mgr = app.autolaunch();
                let currently_on = mgr.is_enabled().unwrap_or(false);
                let result = if currently_on {
                    mgr.disable()
                } else {
                    mgr.enable()
                };
                match result {
                    Ok(()) => {
                        let _ = login_for_cb.set_checked(!currently_on);
                    }
                    Err(e) => eprintln!("DiskSage: could not toggle start-at-login: {e}"),
                }
            }
            _ => {}
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
        // Start-at-login support; the login item is named "DiskSage", not "desktop".
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("DiskSage")
                .build(),
        )
        // Closing the window keeps the app alive in the menu bar (hide, don't quit).
        // The user leaves via the tray's "Quit DiskSage".
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .setup(|app| {
            // macOS: menu-bar-only — no Dock icon, no app-switcher entry.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            if let Err(e) = setup_tray(app) {
                eprintln!("DiskSage: could not create the menu bar tray: {e}");
            }
            // Serve the UI in-process; the thread lives for the life of the app.
            // A bind failure (e.g. the port is already taken by another DiskSage)
            // is logged, not fatal — the window then talks to whatever's on 8765.
            std::thread::spawn(|| {
                if let Err(e) = disksage_engine::serve::run(SERVE_PORT) {
                    eprintln!("DiskSage: serve could not start: {e}");
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building the DiskSage application")
        .run(|_app, _event| {});
}
