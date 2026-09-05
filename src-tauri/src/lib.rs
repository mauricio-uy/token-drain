pub mod cache;
pub mod providers;
pub mod runtime;
pub mod settings;
pub mod tray;
pub mod view;
pub mod window;

use std::sync::Arc;

use tauri::{Emitter, Manager};

use runtime::{
    get_settings, get_usage_snapshot, list_providers, refresh_now, set_settings, UsageState,
    USAGE_UPDATED_EVENT,
};
use settings::SettingsStore;
use window::interaction::{set_interactive_regions, InteractiveRegions};

/// Label of the rail window, matching `tauri.conf.json`.
pub const RAIL_WINDOW_LABEL: &str = "rail";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(InteractiveRegions::default())
        .invoke_handler(tauri::generate_handler![
            set_interactive_regions,
            get_usage_snapshot,
            refresh_now,
            get_settings,
            list_providers,
            set_settings
        ])
        .setup(|app| {
            let data_directory = app.path().app_data_dir()?;

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

            let state: Arc<UsageState> =
                runtime::start(&data_directory, Arc::clone(&settings), move |views| {
                    let _ = emitter.emit(USAGE_UPDATED_EVENT, views);
                })?;

            app.manage(state);

            // After the state is managed, so the tray's Refresh item can reach
            // it the first time it is clicked.
            tray::build(app.handle())?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
