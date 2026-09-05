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
use crate::providers::claude::ClaudeProvider;
use crate::providers::codex::CodexProvider;
use crate::providers::error::UsageError;
use crate::providers::http::build_client;
use crate::providers::refresh::{run_refresh_loop, RefreshConfig};
use crate::providers::registry::{AnyProvider, ProviderFetch, ProviderRegistry};
use crate::providers::usage::{ProviderId, ProviderUsage};
use crate::view::{build_view, ProviderView};

/// The event emitted whenever a poll produces new results.
pub const USAGE_UPDATED_EVENT: &str = "usage-updated";

/// Everything the commands need.
pub struct UsageState {
    /// Providers in display order. The rail renders in this order.
    order: Vec<ProviderId>,
    /// The most recent outcome per provider, live results only.
    latest: Mutex<BTreeMap<ProviderId, Result<ProviderUsage, UsageError>>>,
    cache: Mutex<UsageCache>,
    refresh_now: Arc<Notify>,
}

impl UsageState {
    pub fn new(order: Vec<ProviderId>, cache: UsageCache, refresh_now: Arc<Notify>) -> Self {
        Self {
            order,
            latest: Mutex::new(BTreeMap::new()),
            cache: Mutex::new(cache),
            refresh_now,
        }
    }

    /// The current view for every provider, in display order.
    pub fn views(&self) -> Vec<ProviderView> {
        let latest = self.latest.lock().ok();
        let cache = self.cache.lock().ok();

        self.order
            .iter()
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
}

/// Build the provider registry.
///
/// Both providers are always registered, whether or not their CLI is installed.
/// A missing credentials file is a perfectly good answer — it renders as "not
/// signed in", which is more useful than the provider silently not existing.
pub fn build_registry() -> Result<ProviderRegistry, UsageError> {
    let client = build_client()?;

    Ok(ProviderRegistry::new(vec![
        AnyProvider::Claude(ClaudeProvider::new(client.clone())),
        AnyProvider::Codex(CodexProvider::new(client)),
    ]))
}

/// Display order of the providers, matching [`build_registry`].
pub fn provider_order() -> Vec<ProviderId> {
    vec![ProviderId::Claude, ProviderId::Codex]
}

/// Assemble the state and start the polling loop.
///
/// `on_update` is called with the fresh views after every poll. Passing it in
/// rather than emitting directly keeps this function free of Tauri types and
/// testable.
pub fn start<F>(cache_directory: &Path, on_update: F) -> Result<Arc<UsageState>, UsageError>
where
    F: Fn(Vec<ProviderView>) + Send + 'static,
{
    let registry = Arc::new(build_registry()?);
    let cache = UsageCache::open(cache_directory);
    let refresh_now = Arc::new(Notify::new());

    let state = Arc::new(UsageState::new(
        provider_order(),
        cache,
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
        RefreshConfig::default(),
        sender,
        refresh_now,
    ));

    let consumer_state = Arc::clone(&state);
    tauri::async_runtime::spawn(async move {
        while let Some(batch) = receiver.recv().await {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::usage::{UsageWindow, SESSION_WINDOW_MINUTES};
    use crate::view::BadgeState;

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("tok-ching-runtime-{name}"));
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
            session: UsageWindow::new(used_percent, SESSION_WINDOW_MINUTES, Some(1_788_580_800_000)),
            weekly: None,
            plan: Some("some_plan".to_owned()),
            fetched_at: 1_000,
        }
    }

    fn state_in(dir: &TempDir) -> UsageState {
        UsageState::new(
            provider_order(),
            UsageCache::open(&dir.0),
            Arc::new(Notify::new()),
        )
    }

    #[test]
    fn an_empty_state_reports_every_provider_as_pending() {
        let dir = TempDir::new("pending");
        let state = state_in(&dir);

        let views = state.views();

        assert_eq!(views.len(), 2);
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
            views[0].usage.as_ref().unwrap().session.as_ref().unwrap().used_percent,
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
            views[0].usage.as_ref().unwrap().session.as_ref().unwrap().used_percent,
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
        assert!(views[0].usage.is_none(), "a failure showed a live percentage");
        assert_eq!(
            views[0].last_known.as_ref().unwrap().session.as_ref().unwrap().used_percent,
            55.0
        );
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
