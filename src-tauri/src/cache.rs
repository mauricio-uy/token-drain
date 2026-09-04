//! On-disk cache of the last successful snapshot per provider.
//!
//! Its only job is the first second after launch. Without it the rail comes up
//! empty and stays that way until the first request completes, which reads as a
//! broken widget; with it, the last known figures are on screen immediately,
//! marked stale, and replaced as soon as live data arrives.
//!
//! Security (S1): the cache holds [`ProviderUsage`] values — percentages,
//! window durations, reset timestamps and a plan name. There is no token
//! anywhere in that type, and a test asserts the serialized shape so a field
//! added later cannot quietly start persisting one.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::providers::registry::ProviderFetch;
use crate::providers::usage::{ProviderId, ProviderUsage};

const CACHE_FILE_NAME: &str = "usage-cache.json";

/// Bumped whenever the on-disk shape changes incompatibly. A file carrying any
/// other version is discarded rather than guessed at.
const CACHE_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Deserialize, Serialize)]
struct CacheFile {
    version: u32,
    entries: Vec<ProviderUsage>,
}

/// The last successful snapshot for each provider.
pub struct UsageCache {
    path: PathBuf,
    entries: BTreeMap<ProviderId, ProviderUsage>,
}

impl UsageCache {
    /// Open the cache in a directory, loading whatever is usable.
    ///
    /// **Never fails.** A missing, unreadable, corrupt, or wrong-version file
    /// yields an empty cache. Startup must not depend on a cache being intact:
    /// the cost of an empty one is an empty rail for a few seconds, and the cost
    /// of propagating the error is an app that will not launch.
    pub fn open(directory: &Path) -> Self {
        let path = directory.join(CACHE_FILE_NAME);
        let entries = read_entries(&path).unwrap_or_default();

        Self { path, entries }
    }

    /// The cached snapshots, keyed by provider.
    pub fn entries(&self) -> &BTreeMap<ProviderId, ProviderUsage> {
        &self.entries
    }

    /// The cached snapshot for one provider.
    pub fn get(&self, provider: ProviderId) -> Option<&ProviderUsage> {
        self.entries.get(&provider)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Record one snapshot and persist.
    pub fn record(&mut self, usage: ProviderUsage) -> io::Result<()> {
        self.entries.insert(usage.provider, usage);
        self.persist()
    }

    /// Record the successful outcomes in a batch and persist once.
    ///
    /// Failures are ignored rather than stored. A provider that has started
    /// failing should keep showing its last good figures marked stale, not be
    /// blanked — and it must not lose them because another provider in the same
    /// batch succeeded.
    pub fn record_batch(&mut self, batch: &[ProviderFetch]) -> io::Result<()> {
        let mut changed = false;

        for fetch in batch {
            if let Ok(usage) = &fetch.result {
                self.entries.insert(usage.provider, usage.clone());
                changed = true;
            }
        }

        if changed {
            self.persist()
        } else {
            Ok(())
        }
    }

    /// Write the whole map out atomically.
    ///
    /// Written to a sibling temporary file and renamed into place, so an
    /// interrupted write leaves the previous cache intact rather than a
    /// truncated file that would be discarded on next launch.
    fn persist(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = CacheFile {
            version: CACHE_FORMAT_VERSION,
            entries: self.entries.values().cloned().collect(),
        };

        let serialized = serde_json::to_vec_pretty(&file)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, serialized)?;
        fs::rename(&temporary, &self.path)
    }
}

fn read_entries(path: &Path) -> Option<BTreeMap<ProviderId, ProviderUsage>> {
    let raw = fs::read_to_string(path).ok()?;
    let file: CacheFile = serde_json::from_str(&raw).ok()?;

    if file.version != CACHE_FORMAT_VERSION {
        return None;
    }

    Some(
        file.entries
            .into_iter()
            .map(|usage| (usage.provider, usage))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::error::UsageError;
    use crate::providers::usage::{UsageWindow, SESSION_WINDOW_MINUTES};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("tok-ching-cache-{name}"));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("should create temp dir");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn usage(provider: ProviderId, used_percent: f64, fetched_at: i64) -> ProviderUsage {
        ProviderUsage {
            provider,
            session: UsageWindow::new(used_percent, SESSION_WINDOW_MINUTES, Some(1_788_580_800_000)),
            weekly: None,
            plan: Some("some_plan".to_owned()),
            fetched_at,
        }
    }

    #[test]
    fn survives_a_restart() {
        // The whole point: what was on screen before the app closed is on screen
        // again the moment it reopens.
        let dir = TempDir::new("restart");

        let mut cache = UsageCache::open(dir.path());
        cache
            .record(usage(ProviderId::Claude, 55.0, 1_000))
            .expect("should persist");
        drop(cache);

        let reopened = UsageCache::open(dir.path());

        let restored = reopened.get(ProviderId::Claude).expect("should be cached");
        assert_eq!(restored.session.as_ref().unwrap().used_percent, 55.0);
        assert_eq!(restored.fetched_at, 1_000);
    }

    #[test]
    fn a_partial_poll_does_not_evict_the_other_provider() {
        // Schedules are per provider, so a batch routinely contains only one of
        // them. Writing the batch must merge, not replace.
        let dir = TempDir::new("merge");

        let mut cache = UsageCache::open(dir.path());
        cache.record(usage(ProviderId::Claude, 55.0, 1_000)).unwrap();
        cache.record(usage(ProviderId::Codex, 48.0, 2_000)).unwrap();
        drop(cache);

        let mut cache = UsageCache::open(dir.path());
        cache
            .record_batch(&[ProviderFetch {
                provider: ProviderId::Claude,
                result: Ok(usage(ProviderId::Claude, 60.0, 3_000)),
            }])
            .unwrap();
        drop(cache);

        let reopened = UsageCache::open(dir.path());

        assert_eq!(
            reopened.get(ProviderId::Claude).unwrap().session.as_ref().unwrap().used_percent,
            60.0
        );
        assert_eq!(
            reopened.get(ProviderId::Codex).unwrap().session.as_ref().unwrap().used_percent,
            48.0
        );
    }

    #[test]
    fn a_failed_fetch_does_not_overwrite_good_data() {
        // Why: a provider that starts failing should keep showing its last good
        // figures marked stale. Blanking them loses information the user wants.
        let dir = TempDir::new("failure");

        let mut cache = UsageCache::open(dir.path());
        cache.record(usage(ProviderId::Claude, 55.0, 1_000)).unwrap();

        cache
            .record_batch(&[ProviderFetch {
                provider: ProviderId::Claude,
                result: Err(UsageError::Unauthorized),
            }])
            .unwrap();
        drop(cache);

        let reopened = UsageCache::open(dir.path());

        assert_eq!(
            reopened.get(ProviderId::Claude).unwrap().session.as_ref().unwrap().used_percent,
            55.0
        );
    }

    #[test]
    fn an_all_failure_batch_does_not_touch_the_file() {
        let dir = TempDir::new("no-write");
        let mut cache = UsageCache::open(dir.path());

        cache
            .record_batch(&[ProviderFetch {
                provider: ProviderId::Claude,
                result: Err(UsageError::Unauthorized),
            }])
            .expect("should succeed without writing");

        assert!(!dir.path().join(CACHE_FILE_NAME).exists());
    }

    #[test]
    fn a_missing_cache_opens_empty() {
        let dir = TempDir::new("missing");

        assert!(UsageCache::open(dir.path()).is_empty());
    }

    #[test]
    fn a_corrupt_cache_opens_empty_instead_of_failing() {
        // Startup must never depend on the cache being intact. An empty rail for
        // a few seconds beats an app that will not launch.
        let dir = TempDir::new("corrupt");
        fs::write(dir.path().join(CACHE_FILE_NAME), "{ this is not json").unwrap();

        assert!(UsageCache::open(dir.path()).is_empty());
    }

    #[test]
    fn a_cache_from_a_future_version_is_discarded() {
        let dir = TempDir::new("version");
        fs::write(
            dir.path().join(CACHE_FILE_NAME),
            r#"{"version": 999, "entries": []}"#,
        )
        .unwrap();

        assert!(UsageCache::open(dir.path()).is_empty());
    }

    #[test]
    fn writing_creates_a_missing_directory() {
        let dir = TempDir::new("nested");
        let nested = dir.path().join("deeper").join("still-deeper");

        let mut cache = UsageCache::open(&nested);
        cache
            .record(usage(ProviderId::Claude, 10.0, 1))
            .expect("should create the directory and persist");

        assert!(nested.join(CACHE_FILE_NAME).exists());
    }

    #[test]
    fn writing_leaves_no_temporary_file_behind() {
        // The temporary is renamed into place, not copied and deleted. A
        // leftover would be harmless but signals the atomic write broke.
        let dir = TempDir::new("atomic");

        let mut cache = UsageCache::open(dir.path());
        cache.record(usage(ProviderId::Claude, 10.0, 1)).unwrap();
        cache.record(usage(ProviderId::Codex, 20.0, 2)).unwrap();

        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();

        assert!(leftovers.is_empty(), "left behind {leftovers:?}");
    }

    #[test]
    fn the_persisted_shape_carries_no_credential_material() {
        // Guards S1 structurally: if a token-bearing field is ever added to
        // ProviderUsage, this fails rather than silently persisting it.
        let dir = TempDir::new("s1");

        let mut cache = UsageCache::open(dir.path());
        cache.record(usage(ProviderId::Claude, 55.0, 1_000)).unwrap();

        let written = fs::read_to_string(dir.path().join(CACHE_FILE_NAME)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&written).unwrap();

        let entry = parsed["entries"][0].as_object().expect("an entry object");
        let mut keys: Vec<&str> = entry.keys().map(String::as_str).collect();
        keys.sort_unstable();

        assert_eq!(
            keys,
            ["fetchedAt", "plan", "provider", "session", "weekly"],
            "the cached shape changed; confirm no credential material was added"
        );
    }
}
