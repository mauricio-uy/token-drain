//! Wiring the data layer to the window.
//!
//! Everything below this module is testable in isolation and knows nothing about
//! Tauri. This is where the registry, the cache, the polling loop and the view
//! model are joined together and exposed to the frontend.
//!
//! Security (S4): the two commands here return [`ProviderView`] values only —
//! percentages, window durations, timestamps, a plan name and guidance text. No
//! token ever crosses into the webview, and a test pins the serialized shape so
//! one cannot start to.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, Notify};

use crate::cache::UsageCache;
use crate::notify::{Alert, ThresholdTracker};
use crate::providers::claude::ClaudeProvider;
use crate::providers::codex::CodexProvider;
use crate::providers::error::UsageError;
use crate::providers::http::build_client;
use crate::providers::refresh::{run_refresh_loop, RefreshPolicy};
use crate::providers::registry::{AnyProvider, ProviderFetch, ProviderRegistry};
use crate::providers::usage::{ProviderId, ProviderUsage};
use crate::settings::{Settings, SettingsStore};
use crate::view::{build_view, ProviderView};

/// The event emitted whenever a poll produces new results.
pub const USAGE_UPDATED_EVENT: &str = "usage-updated";

/// Keeps other windows in step with preferences changed in the settings window.
pub const SETTINGS_UPDATED_EVENT: &str = "settings-updated";

/// The settings, seen as the polling loop needs to see them.
///
/// The adapter lives here rather than in the settings module so that neither
/// side has to know about the other: the provider layer sees a policy, the
/// settings module sees a plain preferences file.
struct SettingsPolicy(Arc<SettingsStore>);

impl RefreshPolicy for SettingsPolicy {
    fn interval(&self) -> std::time::Duration {
        self.0.get().poll_interval()
    }

    fn is_enabled(&self, provider: ProviderId) -> bool {
        self.0.get().is_enabled(provider)
    }
}

/// Everything the commands need.
pub struct UsageState {
    /// Providers in display order. The rail renders in this order.
    order: Vec<ProviderId>,
    /// The most recent outcome per provider, live results only.
    latest: Mutex<BTreeMap<ProviderId, Result<ProviderUsage, UsageError>>>,
    cache: Mutex<UsageCache>,
    settings: Arc<SettingsStore>,
    refresh_now: Arc<Notify>,
}

impl UsageState {
    pub fn new(
        order: Vec<ProviderId>,
        cache: UsageCache,
        settings: Arc<SettingsStore>,
        refresh_now: Arc<Notify>,
    ) -> Self {
        Self {
            order,
            latest: Mutex::new(BTreeMap::new()),
            cache: Mutex::new(cache),
            settings,
            refresh_now,
        }
    }

    /// The current view for every enabled provider, in display order.
    ///
    /// A disabled provider is omitted rather than shown in some muted state: it
    /// is not being polled, so any badge for it would be reporting on data the
    /// app has deliberately stopped collecting.
    pub fn views(&self) -> Vec<ProviderView> {
        let latest = self.latest.lock().ok();
        let cache = self.cache.lock().ok();
        let settings = self.settings.get();

        self.order
            .iter()
            .filter(|provider| settings.is_enabled(**provider))
            .map(|provider| {
                let result = latest.as_ref().and_then(|map| map.get(provider));
                let cached = cache.as_ref().and_then(|cache| cache.get(*provider));

                build_view(*provider, result, cached)
            })
            .collect()
    }

    /// Fold a batch of results in: remember them, and persist the successes.
    pub fn apply(&self, batch: Vec<ProviderFetch>) {
        // The cache is written before `latest` is replaced, so a failure never
        // has a window in which neither the new result nor the old cached value
        // is available to render.
        if let Ok(mut cache) = self.cache.lock() {
            let _ = cache.record_batch(&batch);
        }

        if let Ok(mut latest) = self.latest.lock() {
            for fetch in batch {
                latest.insert(fetch.provider, fetch.result);
            }
        }
    }

    /// Ask the polling loop to come back as soon as the floor allows.
    pub fn request_refresh(&self) {
        self.refresh_now.notify_waiters();
    }

    /// Persist preferences, waking providers only when their polling policy changes.
    fn apply_settings(&self, value: Settings) -> std::io::Result<(Settings, Settings)> {
        let previous = self.settings.get();
        let stored = self.settings.set(value)?;
        if previous.poll_interval_seconds != stored.poll_interval_seconds
            || previous.disabled_providers != stored.disabled_providers
        {
            self.request_refresh();
        }
        Ok((previous, stored))
    }
}

/// Build the provider registry.
///
/// Providers are always registered, whether or not credentials are available.
/// A missing credentials file is a perfectly good answer — it renders as "not
/// signed in", which is more useful than the provider silently not existing.
pub fn build_registry() -> Result<ProviderRegistry, UsageError> {
    let client = build_client()?;
    let opencode_client = crate::providers::opencode::build_client()?;

    Ok(ProviderRegistry::new(vec![
        AnyProvider::Claude(ClaudeProvider::new(client.clone())),
        AnyProvider::Codex(CodexProvider::new(client)),
        AnyProvider::OpencodeGo(crate::providers::opencode::go::GoProvider::new(
            opencode_client,
        )),
    ]))
}

/// Display order of the providers, matching [`build_registry`].
pub fn provider_order() -> Vec<ProviderId> {
    vec![
        ProviderId::Claude,
        ProviderId::Codex,
        ProviderId::OpencodeGo,
    ]
}

/// Assemble the state and start the polling loop.
///
/// `on_update` is called with the fresh views after every poll. Passing it in
/// rather than emitting directly keeps this function free of Tauri types and
/// testable.
pub fn start<F, A>(
    cache_directory: &Path,
    settings: Arc<SettingsStore>,
    on_update: F,
    on_alert: A,
) -> Result<Arc<UsageState>, UsageError>
where
    F: Fn(Vec<ProviderView>) + Send + 'static,
    A: Fn(Alert) + Send + 'static,
{
    let registry = Arc::new(build_registry()?);
    let cache = UsageCache::open(cache_directory);
    let refresh_now = Arc::new(Notify::new());

    let state = Arc::new(UsageState::new(
        provider_order(),
        cache,
        Arc::clone(&settings),
        Arc::clone(&refresh_now),
    ));

    let (sender, mut receiver) = mpsc::channel::<Vec<ProviderFetch>>(8);

    // Tauri's async runtime, not `tokio::spawn`: this runs from `setup`, which
    // executes on the main thread before any Tokio reactor is entered, so a bare
    // tokio spawn panics with "there is no reactor running". Keeping the spawn
    // here rather than inside the provider layer also keeps that layer free of
    // Tauri types.
    tauri::async_runtime::spawn(run_refresh_loop(
        registry,
        Arc::new(SettingsPolicy(settings)),
        sender,
        refresh_now,
    ));

    let consumer_state = Arc::clone(&state);
    let alert_settings = Arc::clone(&consumer_state.settings);
    let mut tracker = ThresholdTracker::open(cache_directory);
    tauri::async_runtime::spawn(async move {
        while let Some(batch) = receiver.recv().await {
            // Alerts are decided from the batch, before it is folded in, so the
            // tracker sees each poll exactly once. Reading them back off the
            // state afterwards would also see the cached figures on a failed
            // poll and announce old news as if it had just happened.
            let thresholds = alert_settings.get().active_thresholds();

            if !thresholds.is_empty() {
                for fetch in &batch {
                    if let Ok(usage) = &fetch.result {
                        for alert in tracker.observe(usage, &thresholds) {
                            on_alert(alert);
                        }
                    }
                }
            }

            consumer_state.apply(batch);
            on_update(consumer_state.views());
        }
    });

    Ok(state)
}

/// The current state of every provider.
#[tauri::command]
pub fn get_usage_snapshot(state: tauri::State<'_, Arc<UsageState>>) -> Vec<ProviderView> {
    state.views()
}

/// Ask for an immediate poll. Still subject to the minimum poll interval.
#[tauri::command]
pub fn refresh_now(state: tauri::State<'_, Arc<UsageState>>) {
    state.request_refresh();
}

/// The settings as they stand.
#[tauri::command]
pub fn get_settings(settings: tauri::State<'_, Arc<SettingsStore>>) -> Settings {
    settings.get()
}

/// Every provider the app knows about, enabled or not.
///
/// The settings screen cannot build its list from the usage snapshot, because
/// that deliberately omits disabled providers — a provider switched off would
/// vanish from the only screen that could switch it back on.
#[tauri::command]
pub fn list_providers() -> Vec<ProviderId> {
    provider_order()
}

/// Store new settings and apply them.
///
/// Returns what was actually stored, which may have been clamped. The caller is
/// expected to render the return value rather than what it sent, so the screen
/// always shows the configuration really in force.
///
/// Applying happens here rather than being left to the watchers: the rail
/// re-docks immediately so moving it is direct manipulation rather than a change
/// that shows up a few seconds later, and the poll loop is nudged so a provider
/// switched back on is fetched now instead of at the next deadline.
///
/// The views are re-emitted directly as well. Waiting for the nudged poll to
/// publish them would not do: a manual refresh still obeys the minimum poll
/// interval, so switching a provider off could leave its badge on screen for the
/// best part of a minute. Which providers to *show* is already known here — it
/// needs no network — so it is answered here.
#[tauri::command]
pub fn set_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<UsageState>>,
    value: Settings,
) -> Result<Settings, String> {
    let (previous, stored) = state
        .apply_settings(value)
        .map_err(|error| error.to_string())?;

    if previous.rail_side != stored.rail_side || previous.vertical_offset != stored.vertical_offset
    {
        if let Some(rail) = tauri::Manager::get_webview_window(&app, crate::RAIL_WINDOW_LABEL) {
            crate::window::dock(&rail, &stored);
        }
    }

    if previous.disabled_providers != stored.disabled_providers {
        let _ = tauri::Emitter::emit(&app, USAGE_UPDATED_EVENT, state.views());
    }
    if previous != stored {
        let _ = tauri::Emitter::emit(&app, SETTINGS_UPDATED_EVENT, &stored);
    }

    Ok(stored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::usage::{UsageWindow, SESSION_WINDOW_MINUTES};
    use crate::view::BadgeState;

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("token-drain-runtime-{name}"));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("should create temp dir");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn usage(provider: ProviderId, used_percent: f64) -> ProviderUsage {
        ProviderUsage {
            provider,
            session: UsageWindow::new(
                used_percent,
                SESSION_WINDOW_MINUTES,
                Some(1_788_580_800_000),
            ),
            weekly: None,
            monthly: None,
            plan: Some("some_plan".to_owned()),
            fetched_at: 1_000,
        }
    }

    fn state_in(dir: &TempDir) -> UsageState {
        state_with(dir, Settings::default())
    }

    fn state_with(dir: &TempDir, settings: Settings) -> UsageState {
        let store = Arc::new(SettingsStore::open(&dir.0));
        store.set(settings).expect("should persist");

        UsageState::new(
            provider_order(),
            UsageCache::open(&dir.0),
            store,
            Arc::new(Notify::new()),
        )
    }

    #[tokio::test(start_paused = true)]
    async fn visual_and_alert_edits_persist_without_waking_the_poll_loop() {
        let dir = TempDir::new("visual-preferences");
        let state = state_in(&dir);
        let notified = state.refresh_now.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();

        let edited = Settings {
            vertical_offset: 200,
            rail_side: crate::window::placement::RailSide::Left,
            notifications_enabled: false,
            notification_thresholds: std::collections::BTreeSet::from([50]),
            ..Settings::default()
        };
        state
            .apply_settings(edited.clone())
            .expect("should save preferences");
        assert_eq!(SettingsStore::open(&dir.0).get(), edited);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(1), notified)
                .await
                .is_err()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn polling_edits_wake_the_loop_and_update_provider_visibility() {
        let dir = TempDir::new("polling-preferences");
        let state = state_in(&dir);
        for edited in [
            Settings {
                poll_interval_seconds: 600,
                ..Settings::default()
            },
            Settings {
                poll_interval_seconds: 600,
                disabled_providers: std::collections::BTreeSet::from([ProviderId::Codex]),
                ..Settings::default()
            },
        ] {
            let notified = state.refresh_now.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            state
                .apply_settings(edited)
                .expect("should save preferences");
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(1), notified)
                    .await
                    .is_ok()
            );
        }
        assert_eq!(state.views().len(), provider_order().len() - 1);
        assert_eq!(state.views()[0].provider, ProviderId::Claude);
    }

    #[test]
    fn an_empty_state_reports_every_provider_as_pending() {
        let dir = TempDir::new("pending");
        let state = state_in(&dir);

        let views = state.views();

        assert_eq!(views.len(), provider_order().len());
        assert!(views.iter().all(|view| view.state == BadgeState::Pending));
    }

    #[test]
    fn views_follow_the_configured_display_order() {
        // The rail draws in this order; a state that reordered between polls
        // would make the badges swap places.
        let dir = TempDir::new("order");
        let state = state_in(&dir);

        let providers: Vec<_> = state.views().iter().map(|view| view.provider).collect();

        assert_eq!(providers, provider_order());
    }

    #[test]
    fn applying_a_batch_updates_the_views() {
        let dir = TempDir::new("apply");
        let state = state_in(&dir);

        state.apply(vec![
            ProviderFetch {
                provider: ProviderId::Claude,
                result: Ok(usage(ProviderId::Claude, 55.0)),
            },
            ProviderFetch {
                provider: ProviderId::Codex,
                result: Err(UsageError::Unauthorized),
            },
        ]);

        let views = state.views();

        assert_eq!(views[0].state, BadgeState::Ok);
        assert_eq!(
            views[0]
                .usage
                .as_ref()
                .unwrap()
                .session
                .as_ref()
                .unwrap()
                .used_percent,
            55.0
        );
        assert_eq!(views[1].state, BadgeState::Reauth);
        assert!(views[1].usage.is_none());
    }

    #[test]
    fn a_success_is_persisted_and_survives_a_restart_as_stale() {
        // The Step 3.2 behaviour, exercised through the state that owns it.
        let dir = TempDir::new("restart");

        {
            let state = state_in(&dir);
            state.apply(vec![ProviderFetch {
                provider: ProviderId::Claude,
                result: Ok(usage(ProviderId::Claude, 55.0)),
            }]);
        }

        let restarted = state_in(&dir);
        let views = restarted.views();

        assert_eq!(views[0].state, BadgeState::Stale);
        assert_eq!(
            views[0]
                .usage
                .as_ref()
                .unwrap()
                .session
                .as_ref()
                .unwrap()
                .used_percent,
            55.0
        );
    }

    #[test]
    fn a_later_failure_keeps_the_cached_figures_as_last_known() {
        let dir = TempDir::new("last-known");
        let state = state_in(&dir);

        state.apply(vec![ProviderFetch {
            provider: ProviderId::Claude,
            result: Ok(usage(ProviderId::Claude, 55.0)),
        }]);
        state.apply(vec![ProviderFetch {
            provider: ProviderId::Claude,
            result: Err(UsageError::Unauthorized),
        }]);

        let views = state.views();

        assert_eq!(views[0].state, BadgeState::Reauth);
        assert!(
            views[0].usage.is_none(),
            "a failure showed a live percentage"
        );
        assert_eq!(
            views[0]
                .last_known
                .as_ref()
                .unwrap()
                .session
                .as_ref()
                .unwrap()
                .used_percent,
            55.0
        );
    }

    #[test]
    fn a_disabled_provider_is_left_out_of_the_views() {
        // Not shown greyed out: it is not being polled, so a badge for it would
        // be reporting on data the app has deliberately stopped collecting.
        let dir = TempDir::new("disabled");
        let state = state_with(
            &dir,
            Settings {
                disabled_providers: std::collections::BTreeSet::from([ProviderId::Codex]),
                ..Settings::default()
            },
        );

        let providers: Vec<_> = state.views().iter().map(|view| view.provider).collect();

        assert_eq!(
            providers,
            provider_order()
                .into_iter()
                .filter(|id| *id != ProviderId::Codex)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_disabled_provider_reappears_when_it_is_switched_back_on() {
        // The views are built from the settings on every call rather than at
        // construction, which is what lets the rail update without a restart.
        let dir = TempDir::new("re-enabled");
        let store = Arc::new(SettingsStore::open(&dir.0));
        store
            .set(Settings {
                disabled_providers: std::collections::BTreeSet::from([ProviderId::Codex]),
                ..Settings::default()
            })
            .expect("should persist");

        let state = UsageState::new(
            provider_order(),
            UsageCache::open(&dir.0),
            Arc::clone(&store),
            Arc::new(Notify::new()),
        );

        assert_eq!(state.views().len(), provider_order().len() - 1);

        store.set(Settings::default()).expect("should persist");

        assert_eq!(state.views().len(), provider_order().len());
    }

    #[test]
    fn the_snapshot_sent_to_the_webview_carries_no_credential_material() {
        // Guards S4 structurally. If a token-bearing field is ever added to the
        // view model, this fails rather than shipping it into the webview.
        let dir = TempDir::new("s4");
        let state = state_in(&dir);

        state.apply(vec![ProviderFetch {
            provider: ProviderId::Claude,
            result: Ok(usage(ProviderId::Claude, 55.0)),
        }]);

        let json = serde_json::to_value(state.views()).expect("should serialize");
        let entry = json[0].as_object().expect("a view object");

        let mut keys: Vec<&str> = entry.keys().map(String::as_str).collect();
        keys.sort_unstable();

        assert_eq!(
            keys,
            ["lastKnown", "provider", "remediation", "state", "usage"],
            "the view shape changed; confirm no credential material was added"
        );
    }
}
