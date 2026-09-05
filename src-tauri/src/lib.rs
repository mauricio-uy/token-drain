pub mod autostart;
pub mod cache;
pub mod notify;
pub mod providers;
pub mod runtime;
pub mod settings;
pub mod tray;
pub mod view;
pub mod window;

use std::sync::Arc;

use tauri::{Emitter, Manager};
use tauri_plugin_autostart::MacosLauncher;

use autostart::{get_launch_at_login, set_launch_at_login};
use runtime::{
    get_settings, get_usage_snapshot, list_providers, refresh_now, set_settings, UsageState,
    USAGE_UPDATED_EVENT,
};
use settings::SettingsStore;
use window::interaction::{set_interactive_regions, InteractiveRegions};

/// Label of the rail window, matching `tauri.conf.json`.
pub const RAIL_WINDOW_LABEL: &str = "rail";

/// Where the cache and settings live, with a fallback.
///
/// Resolving this used to be fatal: `app_data_dir()?` from the setup hook meant
/// that on any environment where the path could not be worked out, the whole
/// app panicked and died — no window, no tray icon, no message. Nothing else
/// in this app treats its own storage as essential: the cache opens
/// permissively, the settings fall back to defaults, and both are designed so a
/// widget with no memory still works. Failing to find the directory should be
/// no worse than finding it empty.
///
/// The fallback is under the temp directory, so figures do not survive a reboot
/// there. That is a degraded widget rather than an absent one, which is the
/// right trade for something whose whole job is to be glanceable.
fn data_directory(app: &tauri::AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("tok-ching"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Registered before everything else, as the plugin requires: its whole
        // job is to hand off and exit before a second instance can start
        // building windows, tray icons and pollers it is about to throw away.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // The rail has no focus to give — it is chromeless, always on top
            // and skips the taskbar, so there is no window to raise. Being
            // launched again means "show me", which for this app means making
            // sure the rail is on screen and in the right place.
            if let Some(rail) = app.get_webview_window(RAIL_WINDOW_LABEL) {
                let _ = rail.show();

                if let Some(settings) = app.try_state::<Arc<SettingsStore>>() {
                    window::dock(&rail, &settings.get());
                }
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            // Windows-only app today; the launcher choice only matters on macOS
            // and this is the conventional one to pass.
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(InteractiveRegions::default())
        .invoke_handler(tauri::generate_handler![
            set_interactive_regions,
            get_usage_snapshot,
            refresh_now,
            get_settings,
            list_providers,
            set_settings,
            get_launch_at_login,
            set_launch_at_login
        ])
        .setup(|app| {
            let data_directory = data_directory(app.handle());

            // Settings first: the rail is docked using them, so loading them
            // afterwards would place the window once at the default position
            // and again where the user actually wants it.
            let settings = Arc::new(SettingsStore::open(&data_directory));
            app.manage(Arc::clone(&settings));

            if let Some(rail) = app.get_webview_window(RAIL_WINDOW_LABEL) {
                // Dock once immediately, so the window is never briefly visible
                // at the OS default position before jumping into place.
                window::dock(&rail, &settings.get());
                window::spawn_dock_watcher(rail.clone(), Arc::clone(&settings));
                window::interaction::spawn_cursor_watcher(app.handle().clone(), rail);
            }

            let emitter = app.handle().clone();
            let notifier = app.handle().clone();

            let state: Arc<UsageState> = runtime::start(
                &data_directory,
                Arc::clone(&settings),
                move |views| {
                    let _ = emitter.emit(USAGE_UPDATED_EVENT, views);
                },
                move |alert| {
                    // A toast that fails to send is not worth taking the app
                    // down for, and there is nowhere useful to report it: the
                    // user is by definition not looking at the app.
                    let _ = tauri_plugin_notification::NotificationExt::notification(&notifier)
                        .builder()
                        .title(alert.title())
                        .body(alert.body())
                        .show();
                },
            )?;

            app.manage(state);

            // After the state is managed, so the tray's Refresh item can reach
            // it the first time it is clicked.
            tray::build(app.handle())?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
