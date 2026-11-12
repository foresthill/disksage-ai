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

use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
use tauri_plugin_updater::UpdaterExt;

// Reuse the engine's locale helper so the native menu bar / dialogs match the web
// UI's language (en/ja). Resolved once at startup via `lang::init()`.
use disksage_engine::lang::t;

const TRAY_ID: &str = "disksage-tray";
/// Background poll interval. Language changes are picked up every poll (cheap: an
/// atomic load) so the menu bar follows a Settings language switch quickly; the
/// free-space title is recomputed only every `TITLE_EVERY_N_POLLS` polls (~30s).
const POLL: Duration = Duration::from_secs(3);
const TITLE_EVERY_N_POLLS: u32 = 10;
/// Must match the port the window's index.html polls.
const SERVE_PORT: u16 = 8765;

/// Menu bar title: the startup disk's free space, computed in-process by the
/// DiskSage engine (no subprocess, no protected-folder access → no permission
/// dialogs). Same code path as the CLI, so the numbers always agree.
///
/// Just the number — the tray already shows the DiskSage app icon beside it, so
/// the old 💾 glyph was redundant (and a real icon reads better than an emoji).
fn free_title() -> String {
    disksage_engine::util::human(disksage_engine::df::startup_free())
}

/// Relabel the tray menu items + tooltip for the current language. Called from
/// the background poll when the UI language changes (e.g. via Settings) so the
/// native menu bar stays consistent with the web UI (en/ja). We relabel in place
/// (`set_text`) rather than rebuilding the menu, so the "Start at Login" handler
/// keeps pointing at the live checkbox item. `set_text`/`set_tooltip` dispatch to
/// the main thread internally, so this is safe to call from the poll thread.
fn relabel_tray(
    open_i: &MenuItem<tauri::Wry>,
    check_i: &MenuItem<tauri::Wry>,
    login_i: &CheckMenuItem<tauri::Wry>,
    quit_i: &MenuItem<tauri::Wry>,
    tray: &TrayIcon<tauri::Wry>,
) {
    let _ = open_i.set_text(t("Open DiskSage", "DiskSage を開く"));
    let _ = check_i.set_text(t("Check for Updates…", "アップデートを確認…"));
    let _ = login_i.set_text(t("Start at Login", "ログイン時に起動"));
    let _ = quit_i.set_text(t("Quit DiskSage", "DiskSage を終了"));
    let _ = tray.set_tooltip(Some(t(
        "DiskSage — startup disk free space",
        "DiskSage — 起動ディスクの空き容量",
    )));
}

/// Show and focus the main window (used from the tray "Open" item).
fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Check GitHub for a newer release and, if the user agrees, download + install
/// it, then restart. Driven from the tray because the served UI runs on a
/// different origin (127.0.0.1:8765) and can't call Tauri's updater API.
///
/// `interactive` controls the no-update / error dialogs: on by default from the
/// tray, so the user gets feedback either way. The update's authenticity is
/// verified by the updater's own signature (the `pubkey` in tauri.conf.json),
/// which is separate from Apple/OS code signing.
async fn check_update(app: tauri::AppHandle, interactive: bool) {
    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => {
            if interactive {
                app.dialog()
                    .message(format!("Could not start the update check:\n{e}"))
                    .title("DiskSage")
                    .blocking_show();
            }
            return;
        }
    };
    match updater.check().await {
        Ok(Some(update)) => {
            let version = update.version.clone();
            let proceed =
                app.dialog()
                    .message(format!(
                    "{}\n\n{}",
                    t("A new version", "新しいバージョン").to_string() + &format!(" ({version}) ")
                        + t("is available.", "が公開されています。"),
                    t(
                        "Download and install it now? DiskSage will restart.",
                        "今すぐダウンロードしてインストールしますか？ DiskSage は再起動します。"
                    ),
                ))
                    .title(t(
                        "DiskSage — Update available",
                        "DiskSage — アップデートあり",
                    ))
                    .buttons(MessageDialogButtons::OkCancel)
                    .blocking_show();
            if proceed {
                match update.download_and_install(|_, _| {}, || {}).await {
                    Ok(()) => app.restart(),
                    Err(e) => {
                        app.dialog()
                            .message(format!(
                                "{}\n{e}",
                                t(
                                    "The update could not be installed:",
                                    "アップデートをインストールできませんでした:"
                                )
                            ))
                            .title("DiskSage")
                            .blocking_show();
                    }
                }
            }
        }
        Ok(None) => {
            if interactive {
                app.dialog()
                    .message(t("You're on the latest version.", "最新バージョンです。"))
                    .title("DiskSage")
                    .blocking_show();
            }
        }
        Err(e) => {
            if interactive {
                app.dialog()
                    .message(format!(
                        "{}\n{e}",
                        t("Update check failed:", "アップデート確認に失敗:")
                    ))
                    .title("DiskSage")
                    .blocking_show();
            }
        }
    }
}

/// Build the menu bar tray: free-space title + a small menu
/// (Open / Start at Login / Quit).
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let open_i = MenuItem::with_id(
        app,
        "open",
        t("Open DiskSage", "DiskSage を開く"),
        true,
        None::<&str>,
    )?;
    // Reflect the current login-item state so the checkmark is truthful.
    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);
    let login_i = CheckMenuItem::with_id(
        app,
        "start_at_login",
        t("Start at Login", "ログイン時に起動"),
        true,
        autostart_on,
        None::<&str>,
    )?;
    let check_i = MenuItem::with_id(
        app,
        "check_update",
        t("Check for Updates…", "アップデートを確認…"),
        true,
        None::<&str>,
    )?;
    // A normal MenuItem (not PredefinedMenuItem) so its text can be relabelled at
    // runtime when the language changes; ⌘Q is preserved via the accelerator, and
    // the "quit" id is handled in on_menu_event below.
    let quit_i = MenuItem::with_id(
        app,
        "quit",
        t("Quit DiskSage", "DiskSage を終了"),
        true,
        Some("CmdOrCtrl+Q"),
    )?;
    let menu = Menu::with_items(app, &[&open_i, &check_i, &login_i, &quit_i])?;

    // The toggle updates its own checkmark only when the (un)register succeeds.
    let login_for_cb = login_i.clone();
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .title(free_title())
        .tooltip(t(
            "DiskSage — startup disk free space",
            "DiskSage — 起動ディスクの空き容量",
        ))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "check_update" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(check_update(handle, true));
            }
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
            "quit" => app.exit(0),
            _ => {}
        });
    // An icon is required for the tray to be visible on Windows/Linux (where the
    // free-space *title* text isn't shown); on macOS the icon sits beside it.
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;

    // Background refresh: keep the free-space title current, and relabel the menu
    // + tooltip whenever the UI language changes (e.g. the user switches it in
    // Settings) so the native menu bar stays consistent with the web UI (en/ja).
    let handle = app.handle().clone();
    let open_c = open_i.clone();
    let check_c = check_i.clone();
    let login_c = login_i.clone();
    let quit_c = quit_i.clone();
    std::thread::spawn(move || {
        let mut last_ja = disksage_engine::lang::is_ja();
        let mut tick: u32 = 0;
        loop {
            std::thread::sleep(POLL);
            tick = tick.wrapping_add(1);
            let Some(tray) = handle.tray_by_id(TRAY_ID) else {
                continue;
            };
            if tick.is_multiple_of(TITLE_EVERY_N_POLLS) {
                let _ = tray.set_title(Some(free_title()));
            }
            let now_ja = disksage_engine::lang::is_ja();
            if now_ja != last_ja {
                last_ja = now_ja;
                relabel_tray(&open_c, &check_c, &login_c, &quit_c, &tray);
            }
        }
    });
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // In-app auto-update (tray-driven) + the dialogs it prompts with.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
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
            // Resolve the locale once (config → env → OS) so the native menu bar,
            // tray and updater dialogs match the web UI's language (en/ja).
            disksage_engine::lang::init();
            // macOS: show a Dock icon (so the app icon is visible there) plus a
            // standard menu bar (About / Hide / Quit, Edit, Window…). The app is
            // still menu-bar-resident — closing the window hides it (see below)
            // and a Dock-icon click re-shows it (RunEvent::Reopen in `run`).
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Regular);
                if let Ok(menu) = Menu::default(app.handle()) {
                    let _ = app.set_menu(menu);
                }
            }

            if let Err(e) = setup_tray(app) {
                eprintln!("DiskSage: could not create the menu bar tray: {e}");
            }
            // Tell the engine which released version is running, so its (opt-out)
            // update check can compare against the newest GitHub release. Only the
            // packaged app knows this; the dev CLI leaves it unset and skips the check.
            std::env::set_var("DISKSAGE_VERSION", env!("CARGO_PKG_VERSION"));
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
        .run(|_app, _event| {
            // macOS: re-show the main window when the Dock icon is clicked while
            // no window is visible (closing hides the window to the tray/Dock).
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event {
                if let Some(w) = _app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        });
}
