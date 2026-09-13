//! The tray icon and its menu.
//!
//! The rail has no window chrome, no title bar and no taskbar entry — that is
//! the point, it is furniture rather than an app you switch to. Which leaves the
//! tray as the only place a user can reach it at all, so every control with
//! nowhere else to live belongs here.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

use crate::diagnostics::{self, Event};
use crate::runtime::UsageState;
use crate::settings::SettingsStore;
use crate::window::settings_window;
use crate::RAIL_WINDOW_LABEL;

const MENU_REFRESH: &str = "refresh";
const MENU_VISIBLE: &str = "visible";
const MENU_SETTINGS: &str = "settings";
const MENU_QUIT: &str = "quit";

/// Whether the rail is on screen right now.
///
/// Treats "cannot tell" as hidden: if the window has gone, the honest answer to
/// "is the rail showing" is no.
fn rail_is_visible(app: &AppHandle) -> bool {
    app.get_webview_window(RAIL_WINDOW_LABEL)
        .and_then(|rail| rail.is_visible().ok())
        .unwrap_or(false)
}

fn set_rail_visible(app: &AppHandle, visible: bool) {
    let Some(rail) = app.get_webview_window(RAIL_WINDOW_LABEL) else {
        return;
    };

    if visible {
        if rail.show().is_err() {
            diagnostics::record(Event::OperationFailed {
                operation: diagnostics::Operation::WindowShow,
            });
        }
        // Re-dock on the way back. The work area may have changed while the rail
        // was hidden — a display unplugged, the taskbar moved — and the dock
        // watcher would otherwise leave it in the wrong place for up to its
        // whole interval, which is exactly the moment the user is looking.
        if let Some(settings) = app.try_state::<std::sync::Arc<SettingsStore>>() {
            crate::window::dock(&rail, &settings.get());
        }
    } else {
        if rail.hide().is_err() {
            diagnostics::record(Event::OperationFailed {
                operation: diagnostics::Operation::WindowHide,
            });
        }
    }
}

/// Build the tray icon and wire its menu up to the app.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let refresh = MenuItem::with_id(app, MENU_REFRESH, "Refresh now", true, None::<&str>)?;

    // A checkbox rather than an item whose label flips between "Show" and
    // "Hide": a tick states what is true now, where a verb states what will
    // happen next, and the two read identically at a glance while meaning
    // opposite things.
    let visible = CheckMenuItem::with_id(
        app,
        MENU_VISIBLE,
        "Show the rail",
        true,
        rail_is_visible(app),
        None::<&str>,
    )?;

    let settings = MenuItem::with_id(app, MENU_SETTINGS, "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, MENU_QUIT, "Quit Token Drain", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &refresh,
            &visible,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &quit,
        ],
    )?;

    let checkbox = visible.clone();

    let mut builder = TrayIconBuilder::with_id("token-drain")
        .tooltip("Token Drain")
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            MENU_REFRESH => {
                // Goes through the same request the frontend uses, so it obeys
                // the minimum poll interval rather than giving the tray a way to
                // hammer the providers.
                if let Some(state) = app.try_state::<std::sync::Arc<UsageState>>() {
                    state.request_refresh();
                }
            }
            MENU_VISIBLE => {
                set_rail_visible(app, !rail_is_visible(app));
                // Set the tick from what the window actually did, not from what
                // was asked for. If show() failed, the menu should say so.
                if checkbox.set_checked(rail_is_visible(app)).is_err() {
                    diagnostics::record(Event::OperationFailed {
                        operation: diagnostics::Operation::TrayMenuState,
                    });
                }
            }
            MENU_SETTINGS => {
                if settings_window::open(app).is_err() {
                    diagnostics::record(Event::OperationFailed {
                        operation: diagnostics::Operation::SettingsWindow,
                    });
                }
            }
            MENU_QUIT => app.exit(0),
            _ => {}
        });

    // Without an icon the tray entry still exists but is invisible, which is
    // indistinguishable from the app having failed to start.
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;

    Ok(())
}
