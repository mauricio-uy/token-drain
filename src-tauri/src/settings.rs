//! User settings: what to poll, how often, and where the rail sits.
//!
//! Everything here is a preference, not a fact about the world, so all of it has
//! a sane default. A settings file that is missing, unreadable, truncated or
//! from a future version is not an error condition — it just means the defaults
//! apply. The widget must always come up.
//!
//! Security (S1): like the cache, this file holds no credential material. A test
//! pins the serialized shape so a field carrying one cannot be added quietly.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::providers::schedule::{DEFAULT_POLL_INTERVAL, MIN_POLL_INTERVAL};
use crate::providers::usage::ProviderId;
use crate::window::placement::RailSide;

const SETTINGS_FILE_NAME: &str = "settings.json";

/// Bumped whenever the on-disk shape changes incompatibly. A file carrying any
/// other version is ignored in favour of the defaults rather than guessed at.
const SETTINGS_FORMAT_VERSION: u32 = 1;

/// The slowest poll worth offering. Beyond an hour the figures on screen are old
/// enough to mislead, which is worse than not showing them.
pub const MAX_POLL_INTERVAL: Duration = Duration::from_secs(3_600);

/// How far the rail may be nudged from centre, in logical pixels.
///
/// Bounded rather than free: the offset is a nudge for taste, and the docking
/// rules already keep the window on screen. Allowing an arbitrary number would
/// let a stray digit park the rail against an edge, with no way back except the
/// settings window it just made hard to reach.
pub const MAX_VERTICAL_OFFSET: i32 = 400;

/// Everything the user can change.
///
/// Providers are stored as the **disabled** set rather than the enabled one, so
/// a provider added in a later version arrives switched on. Storing the enabled
/// set would silently hide every new provider from anyone who already has a
/// settings file.
///
/// `#[serde(default)]` is load-bearing: without it, adding a field here would
/// make every existing settings file fail to parse, and the fallback would
/// silently reset the user's whole configuration on upgrade. With it, a missing
/// field takes its default and everything else survives. The format version is
/// for changes that genuinely cannot be read, not for additions.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub poll_interval_seconds: u64,
    pub disabled_providers: BTreeSet<ProviderId>,
    pub rail_side: RailSide,
    pub vertical_offset: i32,
    pub notifications_enabled: bool,
    /// Percentages worth interrupting the user at. A set, so it is inherently
    /// sorted and free of duplicates.
    pub notification_thresholds: BTreeSet<u8>,
    /// Explicit consent for the only optional outbound request the app makes.
    /// When enabled, the rail checks GitHub Releases at startup and installs a
    /// newer package only after the updater has verified its signature.
    pub automatic_updates_enabled: bool,
}

/// Where the defaults come from: 80% is the point at which a long task is worth
/// thinking twice about, and 95% is the point at which one is worth not
/// starting.
pub const DEFAULT_THRESHOLDS: [u8; 2] = [80, 95];

impl Default for Settings {
    fn default() -> Self {
        Self {
            poll_interval_seconds: DEFAULT_POLL_INTERVAL.as_secs(),
            disabled_providers: BTreeSet::new(),
            rail_side: RailSide::default(),
            vertical_offset: 0,
            notifications_enabled: true,
            notification_thresholds: BTreeSet::from(DEFAULT_THRESHOLDS),
            automatic_updates_enabled: false,
        }
    }
}

impl Settings {
    /// Bring every field into range.
    ///
    /// Applied on the way in *and* on the way out, so neither a hand-edited file
    /// nor a frontend bug can put the app into a state its own UI could not
    /// produce. Out-of-range values are clamped rather than rejected: the intent
    /// is clear in every case, just unachievable.
    pub fn sanitised(mut self) -> Self {
        let interval = Duration::from_secs(self.poll_interval_seconds)
            .max(MIN_POLL_INTERVAL)
            .min(MAX_POLL_INTERVAL);
        self.poll_interval_seconds = interval.as_secs();

        self.vertical_offset = self
            .vertical_offset
            .clamp(-MAX_VERTICAL_OFFSET, MAX_VERTICAL_OFFSET);

        // A threshold of 0 would fire the moment a window opened, and one above
        // 100 could never fire at all. Both are dropped rather than clamped:
        // clamping 0 to 1 would invent an intention the user did not have.
        self.notification_thresholds
            .retain(|threshold| (1..=100).contains(threshold));

        self
    }

    /// The thresholds to actually alert on, honouring the on/off switch.
    ///
    /// Switching notifications off is expressed as having nothing to alert on,
    /// so callers have one thing to consult rather than two that could disagree.
    pub fn active_thresholds(&self) -> Vec<u8> {
        if self.notifications_enabled {
            self.notification_thresholds.iter().copied().collect()
        } else {
            Vec::new()
        }
    }

    pub fn poll_interval(&self) -> Duration {
        Duration::from_secs(self.poll_interval_seconds)
    }

    /// Whether a provider should be polled and shown.
    pub fn is_enabled(&self, provider: ProviderId) -> bool {
        !self.disabled_providers.contains(&provider)
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct SettingsFile {
    version: u32,
    settings: Settings,
}

#[derive(Debug, Deserialize)]
struct SettingsFileRead {
    version: u32,
    settings: serde_json::Value,
}

/// The settings, on disk and in memory.
pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    /// Load the settings, falling back to defaults for anything unusable.
    ///
    /// Never fails. There is no version of this app that is more useful for
    /// having refused to start over a settings file.
    pub fn open(directory: &Path) -> Self {
        let path = directory.join(SETTINGS_FILE_NAME);
        let settings = Self::read(&path).unwrap_or_default().sanitised();

        Self {
            path,
            current: Mutex::new(settings),
        }
    }

    fn read(path: &Path) -> Option<Settings> {
        let contents = fs::read_to_string(path).ok()?;
        let mut file: SettingsFileRead = serde_json::from_str(&contents).ok()?;

        if file.version != SETTINGS_FORMAT_VERSION {
            return None;
        }

        if let Some(providers) = file
            .settings
            .get_mut("disabledProviders")
            .and_then(serde_json::Value::as_array_mut)
        {
            providers.retain(|value| {
                value
                    .as_str()
                    .is_some_and(|provider| ProviderId::parse(provider).is_some())
            });
        }

        serde_json::from_value(file.settings).ok()
    }

    /// The settings as they stand.
    pub fn get(&self) -> Settings {
        match self.current.lock() {
            Ok(current) => current.clone(),
            // A poisoned lock means another thread panicked mid-update. The
            // defaults are still a working configuration, which is the point.
            Err(_) => Settings::default(),
        }
    }

    /// Replace the settings and write them out.
    ///
    /// Returns what was actually stored, which may differ from what was asked
    /// for. The caller should render the returned values rather than its own, so
    /// the UI shows the configuration really in force.
    pub fn set(&self, settings: Settings) -> io::Result<Settings> {
        let settings = settings.sanitised();

        if let Ok(mut current) = self.current.lock() {
            *current = settings.clone();
        }

        self.persist(&settings)?;

        Ok(settings)
    }

    /// Write the settings out atomically.
    ///
    /// Written to a sibling temporary file and renamed into place, so an
    /// interrupted write leaves the previous settings intact rather than a
    /// truncated file that would silently reset every preference.
    fn persist(&self, settings: &Settings) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = SettingsFile {
            version: SETTINGS_FORMAT_VERSION,
            settings: settings.clone(),
        };

        let json = serde_json::to_string_pretty(&file)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, json)?;
        fs::rename(&temporary, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("token-drain-settings-{name}"));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("should create temp dir");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn an_absent_file_yields_the_defaults() {
        let dir = TempDir::new("absent");

        let store = SettingsStore::open(&dir.0);

        assert_eq!(store.get(), Settings::default());
    }

    #[test]
    fn settings_survive_a_restart() {
        // The whole point of the step: every setting comes back.
        let dir = TempDir::new("restart");

        let wanted = Settings {
            poll_interval_seconds: 900,
            disabled_providers: BTreeSet::from([ProviderId::Codex]),
            rail_side: RailSide::Left,
            vertical_offset: -120,
            notifications_enabled: false,
            notification_thresholds: BTreeSet::from([50, 90]),
            automatic_updates_enabled: true,
        };

        SettingsStore::open(&dir.0)
            .set(wanted.clone())
            .expect("should persist");

        assert_eq!(SettingsStore::open(&dir.0).get(), wanted);
    }

    #[test]
    fn a_corrupt_file_falls_back_to_the_defaults_rather_than_failing() {
        let dir = TempDir::new("corrupt");
        fs::write(dir.0.join(SETTINGS_FILE_NAME), "{ not json").expect("should write");

        assert_eq!(SettingsStore::open(&dir.0).get(), Settings::default());
    }

    #[test]
    fn a_file_from_another_format_version_is_ignored() {
        let dir = TempDir::new("version");
        let contents = concat!(
            r#"{"version":999,"settings":{"pollIntervalSeconds":60,"#,
            r#""disabledProviders":[],"railSide":"right","verticalOffset":0}}"#
        );
        fs::write(dir.0.join(SETTINGS_FILE_NAME), contents).expect("should write");

        assert_eq!(SettingsStore::open(&dir.0).get(), Settings::default());
    }

    #[test]
    fn a_poll_interval_below_the_floor_is_raised_to_it() {
        // The floor exists to keep us from hammering the providers. A settings
        // file must not be a way around it.
        let settings = Settings {
            poll_interval_seconds: 1,
            ..Settings::default()
        }
        .sanitised();

        assert_eq!(settings.poll_interval(), MIN_POLL_INTERVAL);
    }

    #[test]
    fn an_absurd_poll_interval_is_capped() {
        let settings = Settings {
            poll_interval_seconds: 86_400,
            ..Settings::default()
        }
        .sanitised();

        assert_eq!(settings.poll_interval(), MAX_POLL_INTERVAL);
    }

    #[test]
    fn a_hand_edited_file_cannot_put_the_app_out_of_range() {
        // Sanitising on read, not only on write, is what makes this true.
        let dir = TempDir::new("hand-edited");
        let contents = concat!(
            r#"{"version":1,"settings":{"pollIntervalSeconds":2,"#,
            r#""disabledProviders":[],"railSide":"left","verticalOffset":99999}}"#
        );
        fs::write(dir.0.join(SETTINGS_FILE_NAME), contents).expect("should write");

        let settings = SettingsStore::open(&dir.0).get();

        assert_eq!(settings.poll_interval(), MIN_POLL_INTERVAL);
        assert_eq!(settings.vertical_offset, MAX_VERTICAL_OFFSET);
        // Values that were already in range are kept, not reset along with the
        // ones that were not.
        assert_eq!(settings.rail_side, RailSide::Left);
    }

    #[test]
    fn setting_returns_what_was_actually_stored() {
        let dir = TempDir::new("echo");
        let store = SettingsStore::open(&dir.0);

        let stored = store
            .set(Settings {
                vertical_offset: 10_000,
                ..Settings::default()
            })
            .expect("should persist");

        assert_eq!(stored.vertical_offset, MAX_VERTICAL_OFFSET);
        assert_eq!(store.get(), stored);
    }

    #[test]
    fn a_provider_missing_from_the_file_is_enabled() {
        // Storing the disabled set is what makes a provider added in a later
        // version arrive switched on for existing users.
        let settings = Settings::default();

        assert!(settings.is_enabled(ProviderId::Claude));
        assert!(settings.is_enabled(ProviderId::Codex));
    }

    #[test]
    fn a_disabled_provider_reads_as_disabled() {
        let settings = Settings {
            disabled_providers: BTreeSet::from([ProviderId::Claude]),
            ..Settings::default()
        };

        assert!(!settings.is_enabled(ProviderId::Claude));
        assert!(settings.is_enabled(ProviderId::Codex));
    }

    #[test]
    fn the_settings_file_carries_no_credential_material() {
        // Guards S1 for this file the way the cache test does for its own.
        let json = serde_json::to_value(Settings::default()).expect("should serialize");

        let mut keys: Vec<&str> = json
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();

        assert_eq!(
            keys,
            [
                "automaticUpdatesEnabled",
                "disabledProviders",
                "notificationThresholds",
                "notificationsEnabled",
                "pollIntervalSeconds",
                "railSide",
                "verticalOffset"
            ],
            "the settings shape changed; confirm no credential material was added"
        );
    }

    #[test]
    fn a_file_written_before_a_field_existed_keeps_everything_else() {
        // Upgrading must not quietly reset someone's configuration. Without
        // `#[serde(default)]` the whole file fails to parse over one missing
        // field and every preference falls back to its default.
        let dir = TempDir::new("older-version");
        let contents = concat!(
            r#"{"version":1,"settings":{"pollIntervalSeconds":900,"#,
            r#""disabledProviders":["codex"],"railSide":"left","verticalOffset":-120}}"#
        );
        fs::write(dir.0.join(SETTINGS_FILE_NAME), contents).expect("should write");

        let settings = SettingsStore::open(&dir.0).get();

        assert_eq!(settings.poll_interval_seconds, 900);
        assert_eq!(settings.rail_side, RailSide::Left);
        assert_eq!(settings.vertical_offset, -120);
        assert!(!settings.is_enabled(ProviderId::Codex));
        // The fields that did not exist yet take their defaults.
        assert!(settings.notifications_enabled);
        assert_eq!(
            settings.notification_thresholds,
            BTreeSet::from(DEFAULT_THRESHOLDS)
        );
    }

    #[test]
    fn an_unrecognized_disabled_provider_preserves_other_preferences() {
        let dir = TempDir::new("retired-provider");
        let contents = concat!(
            r#"{"version":1,"settings":{"pollIntervalSeconds":900,"#,
            r#""disabledProviders":["codex","retired-provider"],"railSide":"left","verticalOffset":-120}}"#
        );
        fs::write(dir.0.join(SETTINGS_FILE_NAME), contents).expect("should write");

        let settings = SettingsStore::open(&dir.0).get();

        assert_eq!(settings.poll_interval_seconds, 900);
        assert_eq!(settings.rail_side, RailSide::Left);
        assert!(!settings.is_enabled(ProviderId::Codex));
        assert_eq!(settings.disabled_providers.len(), 1);
    }

    #[test]
    fn nonsense_thresholds_are_dropped_rather_than_clamped() {
        // Clamping 0 to 1 would invent an intention the user did not have.
        let settings = Settings {
            notification_thresholds: BTreeSet::from([0, 80, 101, 255]),
            ..Settings::default()
        }
        .sanitised();

        assert_eq!(
            settings.notification_thresholds,
            BTreeSet::from([80]),
            "an unusable threshold survived"
        );
    }

    #[test]
    fn switching_notifications_off_leaves_nothing_to_alert_on() {
        let settings = Settings {
            notifications_enabled: false,
            ..Settings::default()
        };

        assert!(settings.active_thresholds().is_empty());
        // The chosen thresholds are kept, so switching back on restores them
        // rather than resetting to the defaults.
        assert_eq!(
            settings.notification_thresholds,
            BTreeSet::from(DEFAULT_THRESHOLDS)
        );
    }

    #[test]
    fn active_thresholds_come_back_in_order() {
        let settings = Settings {
            notification_thresholds: BTreeSet::from([95, 50, 80]),
            ..Settings::default()
        };

        assert_eq!(settings.active_thresholds(), vec![50, 80, 95]);
    }
}
