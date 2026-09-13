//! Bounded, structured diagnostics for the native application.
//!
//! The logger deliberately accepts an enum rather than a string. This keeps
//! credentials, HTTP headers, response bodies, and other free-form data out of
//! the diagnostic boundary by construction.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use tauri::Manager;

use crate::providers::error::{NetworkFailure, UsageError};
use crate::providers::usage::ProviderId;
use crate::view::BadgeState;
use crate::window::placement::RailSide;

/// The version shown in Settings and written to every fresh log file.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

const LOG_DIRECTORY_NAME: &str = "logs";
const LOG_FILE_NAME: &str = "token-drain.log";
const MAX_FILE_BYTES: u64 = 64 * 1024;
const MAX_BACKUP_FILES: usize = 3;

/// Runtime operations whose failures are useful to diagnose.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Operation {
    Notification,
    UsageUpdate,
    CacheWrite,
    AlertPersistence,
    SettingsPersistence,
    SettingsUpdate,
    WindowShow,
    WindowHide,
    WindowFocus,
    WindowPosition,
    CursorEvents,
    CursorPosition,
    TrayMenuState,
    SettingsWindow,
    LogWrite,
}

/// Coarse provider failure information. No source error or request detail is
/// retained, because those values can include credential or endpoint data.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailureKind {
    Unauthorized,
    RateLimited,
    Server,
    Timeout,
    Connect,
    Request,
    Parse,
    MissingCredentials,
}

/// A value accepted by the logger. Every field is a fixed, reviewable type.
#[derive(Debug, Serialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum Event {
    Startup,
    Poll {
        provider: ProviderId,
        outcome: PollOutcome,
        duration_ms: u64,
        badge_state: BadgeState,
        http_status: Option<u16>,
    },
    SettingsChanged,
    NotificationSent {
        provider: ProviderId,
        window: crate::notify::WindowKind,
        threshold: u8,
    },
    DockRecalculated {
        side: RailSide,
        x: i32,
        y: i32,
    },
    OperationFailed {
        operation: Operation,
    },
}

/// The outcome of one provider poll, without carrying the original error.
#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PollOutcome {
    Success,
    Failure { kind: FailureKind },
}

#[derive(Serialize)]
struct Envelope<'a> {
    version: &'static str,
    timestamp: String,
    #[serde(flatten)]
    event: &'a Event,
}

struct LogState {
    file: Option<File>,
    bytes: u64,
}

/// Thread-safe logger with a fixed-size current file and a small backup set.
pub struct Diagnostics {
    directory: PathBuf,
    state: Mutex<LogState>,
}

impl Diagnostics {
    /// Open the logger under the supplied app-data directory. Logging is best
    /// effort: a filesystem problem must never prevent the widget from using
    /// its providers.
    pub fn open(data_directory: &Path) -> Self {
        let directory = log_directory(data_directory);
        let _ = fs::create_dir_all(&directory);
        let (mut file, mut bytes) = open_current(&directory);
        if bytes == 0 {
            bytes = write_header(&mut file);
        }
        Self {
            directory,
            state: Mutex::new(LogState { file, bytes }),
        }
    }

    /// Record one event. Serialization is performed only from the fixed event
    /// enum, never from caller-provided text.
    pub fn record(&self, event: Event) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let envelope = Envelope {
            version: APP_VERSION,
            timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            event: &event,
        };
        let Ok(mut line) = serde_json::to_vec(&envelope) else {
            return;
        };
        line.push(b'\n');

        if state.bytes > 0 && state.bytes.saturating_add(line.len() as u64) > MAX_FILE_BYTES {
            self.rotate(&mut state);
        }

        let Some(file) = state.file.as_mut() else {
            return;
        };
        if file.write_all(&line).is_ok() && file.flush().is_ok() {
            state.bytes = state.bytes.saturating_add(line.len() as u64);
        }
    }

    /// The directory presented to the user by the Settings action.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    fn rotate(&self, state: &mut LogState) {
        // Close the handle before renaming. Windows does not permit renaming an
        // open file, and failing to rotate would silently defeat the bound.
        state.file.take();
        for index in (1..=MAX_BACKUP_FILES).rev() {
            let source = backup_path(&self.directory, index - 1);
            let target = backup_path(&self.directory, index);
            let _ = fs::remove_file(&target);
            let _ = fs::rename(source, target);
        }

        let current = self.directory.join(LOG_FILE_NAME);
        let _ = fs::remove_file(&current);
        let (mut file, mut bytes) = open_current(&self.directory);
        if bytes == 0 {
            bytes = write_header(&mut file);
        }
        state.file = file;
        state.bytes = bytes;
    }
}

/// Initialize the process logger once and return its shared instance.
pub fn initialize(data_directory: &Path) -> &'static Diagnostics {
    GLOBAL.get_or_init(|| Diagnostics::open(data_directory))
}

/// Record an event when the application logger has been initialized.
pub fn record(event: Event) {
    if let Some(logger) = GLOBAL.get() {
        logger.record(event);
    }
}

/// Resolve the stable app-data subdirectory used for diagnostics.
pub fn log_directory(data_directory: &Path) -> PathBuf {
    data_directory.join(LOG_DIRECTORY_NAME)
}

/// Return the version compiled into the native package.
#[tauri::command]
pub fn get_app_version() -> &'static str {
    APP_VERSION
}

/// Open the fixed diagnostics directory in Explorer.
///
/// The command does not accept a path from the webview and refuses calls from
/// any window other than Settings. This keeps the action useful while avoiding
/// a general-purpose path opener in the renderer boundary.
#[tauri::command]
pub fn open_log_directory(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    if window.label() != crate::window::settings_window::SETTINGS_WINDOW_LABEL {
        return Err("Only the settings window can open diagnostics.".to_owned());
    }

    let data_directory = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("token-drain"));
    let directory = log_directory(&data_directory);
    std::fs::create_dir_all(&directory)
        .map_err(|_| "Could not create the diagnostics directory.".to_owned())?;
    tauri_plugin_opener::OpenerExt::opener(&app)
        .open_path(directory.to_string_lossy().to_string(), None::<&str>)
        .map_err(|_| "Could not open the diagnostics directory.".to_owned())
}

/// Map a classified provider error to a safe diagnostic category.
pub fn failure_kind(error: &UsageError) -> FailureKind {
    match error {
        UsageError::Unauthorized => FailureKind::Unauthorized,
        UsageError::RateLimited { .. } => FailureKind::RateLimited,
        UsageError::Server { .. } => FailureKind::Server,
        UsageError::Network { reason } => match reason {
            NetworkFailure::Timeout => FailureKind::Timeout,
            NetworkFailure::Connect => FailureKind::Connect,
            NetworkFailure::Request => FailureKind::Request,
        },
        UsageError::Parse => FailureKind::Parse,
        UsageError::MissingCredentials(_) => FailureKind::MissingCredentials,
    }
}

/// Extract only an HTTP status code from a classified error.
pub fn http_status(error: &UsageError) -> Option<u16> {
    match error {
        UsageError::Unauthorized => Some(401),
        UsageError::RateLimited { .. } => Some(429),
        UsageError::Server { status } => Some(*status),
        _ => None,
    }
}

fn backup_path(directory: &Path, index: usize) -> PathBuf {
    if index == 0 {
        directory.join(LOG_FILE_NAME)
    } else {
        directory.join(format!("{LOG_FILE_NAME}.{index}"))
    }
}

fn open_current(directory: &Path) -> (Option<File>, u64) {
    let path = directory.join(LOG_FILE_NAME);
    let bytes = fs::metadata(&path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if bytes >= MAX_FILE_BYTES {
        let _ = fs::remove_file(&path);
    }
    let bytes = if bytes >= MAX_FILE_BYTES { 0 } else { bytes };
    let file = OpenOptions::new().create(true).append(true).open(path).ok();
    let bytes = file
        .as_ref()
        .and_then(|_| fs::metadata(directory.join(LOG_FILE_NAME)).ok())
        .map(|metadata| metadata.len())
        .unwrap_or(bytes);
    (file, bytes)
}

fn write_header(file: &mut Option<File>) -> u64 {
    let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let line =
        format!(r#"{{"version":"{APP_VERSION}","timestamp":"{timestamp}","event":"log-started"}}"#)
            + "\n";
    let bytes = line.len() as u64;
    if file
        .as_mut()
        .is_some_and(|file| file.write_all(line.as_bytes()).is_ok() && file.flush().is_ok())
    {
        bytes
    } else {
        0
    }
}

static GLOBAL: OnceLock<Diagnostics> = OnceLock::new();

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_directory(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("token-drain-diagnostics-{name}"));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("should create diagnostics directory");
        path
    }

    #[test]
    fn a_fresh_log_starts_with_the_build_version() {
        let logger = Diagnostics::open(&temp_directory("version"));
        logger.record(Event::Startup);

        let contents =
            fs::read_to_string(logger.directory().join(LOG_FILE_NAME)).expect("log should exist");
        let first_line = contents
            .lines()
            .next()
            .expect("log should have a first line");
        assert!(first_line.contains(APP_VERSION));
        logger.record(Event::SettingsChanged);
        let lines: Vec<_> = fs::read_to_string(logger.directory().join(LOG_FILE_NAME))
            .expect("log should remain readable")
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("valid JSON line"))
            .collect();
        assert!(lines.iter().all(|line| line["version"] == APP_VERSION));
        assert!(lines.iter().all(|line| line["timestamp"].is_string()));
    }

    #[test]
    fn rotation_keeps_a_bounded_backup_set() {
        let logger = Diagnostics::open(&temp_directory("rotation"));
        for _ in 0..2_000 {
            logger.record(Event::OperationFailed {
                operation: Operation::CacheWrite,
            });
        }

        let entries: Vec<_> = fs::read_dir(logger.directory())
            .expect("log directory should be readable")
            .filter_map(Result::ok)
            .collect();
        assert!(entries.len() <= MAX_BACKUP_FILES + 1);
        for entry in entries {
            assert!(
                entry.metadata().expect("metadata should be readable").len() <= MAX_FILE_BYTES,
                "a rotated log exceeded its cap"
            );
        }
    }

    #[test]
    fn classified_error_mapping_does_not_render_error_text() {
        let error = UsageError::Server { status: 503 };
        let event = Event::Poll {
            provider: ProviderId::Claude,
            outcome: PollOutcome::Failure {
                kind: failure_kind(&error),
            },
            duration_ms: 1,
            badge_state: BadgeState::Unavailable,
            http_status: http_status(&error),
        };
        let encoded = serde_json::to_string(&event).expect("event should serialize");
        assert!(!encoded.contains("provider returned"));
        assert!(encoded.contains("503"));
    }
}
