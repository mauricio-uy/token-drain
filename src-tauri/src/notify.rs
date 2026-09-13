//! Deciding when a quota is worth interrupting someone about.
//!
//! Pure logic, no Tauri: given what a provider just reported and what has
//! already been announced, this works out which toast — if any — to fire. The
//! sending lives in `lib.rs`; everything difficult is here, where it can be
//! tested.
//!
//! The hard part is not the threshold, it is the memory. A widget that toasts
//! on every poll above 80% would fire twelve times an hour and be muted within a
//! day, which would also mute the 95% warning that actually matters.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::diagnostics::{self, Event};

use crate::providers::usage::{ProviderId, ProviderUsage, UsageWindow};

const ALERTS_FILE_NAME: &str = "alerts.json";
const ALERTS_FORMAT_VERSION: u32 = 1;

/// How far a reset time may move before it counts as a different quota period.
///
/// Reset timestamps are not stable. At least one provider computes them
/// relative to the moment of the request, so the same window comes back a
/// second or so later on every poll. Comparing them exactly would make every
/// poll look like a new period, wipe the record of what had been announced, and
/// toast again — every poll, for as long as the quota stayed high. Which is the
/// exact failure this whole module exists to prevent.
///
/// Two minutes is far above that jitter and far below a real rollover, which
/// moves the reset forward by the length of the window: hours, at least.
const PERIOD_DRIFT_TOLERANCE_MS: i64 = 120_000;

/// Whether two reset times describe the same quota period.
fn same_period(left: Option<i64>, right: Option<i64>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => (left - right).abs() <= PERIOD_DRIFT_TOLERANCE_MS,
        // Neither reported one: nothing says the period changed, so treat it as
        // continuing. The drop-detection below is what catches a rollover here.
        (None, None) => true,
        // One reported a time and the other did not. Something genuinely
        // changed about the window; treat it as new.
        _ => false,
    }
}

/// Which of a provider's windows an alert is about.
///
/// Tracked separately because they reset on completely different schedules: a
/// weekly window at 95% is news even when the session window is at nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WindowKind {
    Session,
    Weekly,
    Monthly,
}

impl WindowKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
        }
    }
}

/// One thing worth telling the user.
#[derive(Debug, Clone, PartialEq)]
pub struct Alert {
    pub provider: ProviderId,
    pub window: WindowKind,
    pub threshold: u8,
    pub used_percent: f64,
}

impl Alert {
    pub fn title(&self) -> String {
        format!(
            "{} — {} {}% used",
            display_name(self.provider),
            self.window.as_str(),
            self.used_percent.round() as i64
        )
    }

    pub fn body(&self) -> String {
        format!("Past the {}% mark.", self.threshold)
    }
}

fn display_name(provider: ProviderId) -> &'static str {
    match provider {
        ProviderId::Claude => "Claude",
        ProviderId::Codex => "Codex",
        ProviderId::OpencodeGo => "OpenCode Go",
    }
}

/// What has already been said about one provider's window.
#[derive(Debug, Default)]
struct Announced {
    /// The reset timestamp this record belongs to. A different one means a
    /// different quota period, and nothing said about the old one still applies.
    period: Option<i64>,
    thresholds: BTreeSet<u8>,
}

/// One remembered record, in the shape it takes on disk.
///
/// A list rather than a map because the key is a pair, which JSON cannot use as
/// an object key without inventing an encoding for it.
#[derive(Debug, Deserialize, Serialize)]
struct AlertRecord {
    provider: ProviderId,
    window: WindowKind,
    period: Option<i64>,
    thresholds: BTreeSet<u8>,
}

#[derive(Debug, Deserialize, Serialize)]
struct AlertFile {
    version: u32,
    records: Vec<AlertRecord>,
}

/// Remembers what has been announced, so nothing is said twice.
///
/// The memory outlives the process. "Once per quota period" has to mean once
/// per quota period, not once per launch: a five-hour window with the app
/// restarted three times inside it should still produce one toast, not three.
#[derive(Debug, Default)]
pub struct ThresholdTracker {
    seen: BTreeMap<(ProviderId, WindowKind), Announced>,
    path: Option<PathBuf>,
}

impl ThresholdTracker {
    /// An in-memory tracker that forgets when the process ends.
    pub fn new() -> Self {
        Self::default()
    }

    /// A tracker backed by a file, so what has been said survives a restart.
    ///
    /// Never fails. Losing this memory means at worst one repeated toast, which
    /// is not worth refusing to start over.
    pub fn open(directory: &Path) -> Self {
        let path = directory.join(ALERTS_FILE_NAME);

        Self {
            seen: Self::read(&path).unwrap_or_default(),
            path: Some(path),
        }
    }

    fn read(path: &Path) -> Option<BTreeMap<(ProviderId, WindowKind), Announced>> {
        let contents = fs::read_to_string(path).ok()?;
        let file: AlertFile = serde_json::from_str(&contents).ok()?;

        if file.version != ALERTS_FORMAT_VERSION {
            return None;
        }

        Some(
            file.records
                .into_iter()
                .map(|record| {
                    (
                        (record.provider, record.window),
                        Announced {
                            period: record.period,
                            thresholds: record.thresholds,
                        },
                    )
                })
                .collect(),
        )
    }

    /// Write the memory out atomically, the way the cache does.
    fn persist(&self) {
        let Some(path) = &self.path else {
            return;
        };

        let file = AlertFile {
            version: ALERTS_FORMAT_VERSION,
            records: self
                .seen
                .iter()
                .map(|((provider, window), announced)| AlertRecord {
                    provider: *provider,
                    window: *window,
                    period: announced.period,
                    thresholds: announced.thresholds.clone(),
                })
                .collect(),
        };

        let Ok(json) = serde_json::to_string_pretty(&file) else {
            diagnostics::record(Event::OperationFailed {
                operation: diagnostics::Operation::AlertPersistence,
            });
            return;
        };

        let temporary = path.with_extension("json.tmp");
        let failed = fs::write(&temporary, json).is_err() || fs::rename(&temporary, path).is_err();
        if failed {
            diagnostics::record(Event::OperationFailed {
                operation: diagnostics::Operation::AlertPersistence,
            });
        }
    }

    /// Alerts for everything this snapshot newly crossed.
    pub fn observe(&mut self, usage: &ProviderUsage, thresholds: &[u8]) -> Vec<Alert> {
        let mut alerts = Vec::new();

        let windows = [
            (WindowKind::Session, usage.session.as_ref()),
            (WindowKind::Weekly, usage.weekly.as_ref()),
            (WindowKind::Monthly, usage.monthly.as_ref()),
        ];

        let before = self.fingerprint();

        for (kind, window) in windows {
            if let Some(window) = window {
                if let Some(alert) = self.observe_window(usage.provider, kind, window, thresholds) {
                    alerts.push(alert);
                }
            }
        }

        // Written only when the memory actually moved. Most polls change
        // nothing, and this runs on every one of them.
        if self.fingerprint() != before {
            self.persist();
        }

        alerts
    }

    /// A cheap stand-in for "has anything changed", used to avoid pointless
    /// writes. Comparing the records directly would need `Clone` on the whole
    /// map for the same answer.
    fn fingerprint(&self) -> Vec<(ProviderId, WindowKind, Option<i64>, usize)> {
        self.seen
            .iter()
            .map(|((provider, window), announced)| {
                (
                    *provider,
                    *window,
                    announced.period,
                    announced.thresholds.len(),
                )
            })
            .collect()
    }

    fn observe_window(
        &mut self,
        provider: ProviderId,
        kind: WindowKind,
        window: &UsageWindow,
        thresholds: &[u8],
    ) -> Option<Alert> {
        let record = self.seen.entry((provider, kind)).or_default();

        // A new quota period wipes the slate. Two ways to notice one: the reset
        // timestamp moved, or — for providers that report no timestamp at all —
        // the figure fell back below something already announced, which only
        // happens when the quota has rolled over.
        let rolled_over = !same_period(record.period, window.resets_at)
            || record
                .thresholds
                .iter()
                .next()
                .is_some_and(|lowest| window.used_percent < f64::from(*lowest));

        if rolled_over {
            record.thresholds.clear();
        }

        // The stored timestamp follows the latest reading either way, so drift
        // accumulates against the newest value rather than the first one ever
        // seen — which would eventually exceed the tolerance on its own.
        record.period = window.resets_at;

        let crossed: Vec<u8> = thresholds
            .iter()
            .copied()
            .filter(|threshold| window.used_percent >= f64::from(*threshold))
            .filter(|threshold| !record.thresholds.contains(threshold))
            .collect();

        let highest = crossed.iter().copied().max()?;

        // Everything crossed is marked, not just the one announced. Jumping from
        // 30% straight to 96% should say 95% once — not 95% now and a stale 80%
        // at the next poll.
        record.thresholds.extend(crossed);

        Some(Alert {
            provider,
            window: kind,
            threshold: highest,
            used_percent: window.used_percent,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monthly_alerts_are_independent_and_deduplicated() {
        let mut snapshot = usage(10.0, Some(1000));
        snapshot.monthly = UsageWindow::new(95.0, 43_200, Some(2_000_000));
        let mut tracker = ThresholdTracker::new();
        let alerts = tracker.observe(&snapshot, &[80]);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].window, WindowKind::Monthly);
        assert!(tracker.observe(&snapshot, &[80]).is_empty());
    }
    use crate::providers::usage::{SESSION_WINDOW_MINUTES, WEEKLY_WINDOW_MINUTES};

    const THRESHOLDS: [u8; 2] = [80, 95];
    const PERIOD: i64 = 1_788_580_800_000;

    fn usage(session_percent: f64, resets_at: Option<i64>) -> ProviderUsage {
        ProviderUsage {
            provider: ProviderId::Claude,
            session: UsageWindow::new(session_percent, SESSION_WINDOW_MINUTES, resets_at),
            weekly: None,
            monthly: None,
            plan: None,
            fetched_at: 0,
        }
    }

    #[test]
    fn nothing_is_said_below_the_lowest_threshold() {
        let mut tracker = ThresholdTracker::new();

        assert!(tracker
            .observe(&usage(79.0, Some(PERIOD)), &THRESHOLDS)
            .is_empty());
    }

    #[test]
    fn crossing_a_threshold_fires_once() {
        let mut tracker = ThresholdTracker::new();

        let alerts = tracker.observe(&usage(81.0, Some(PERIOD)), &THRESHOLDS);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].threshold, 80);
    }

    #[test]
    fn re_polling_at_the_same_level_does_not_fire_again() {
        // The whole point. Without this the widget toasts on every poll for as
        // long as the quota stays high, and gets muted.
        let mut tracker = ThresholdTracker::new();

        assert_eq!(
            tracker
                .observe(&usage(81.0, Some(PERIOD)), &THRESHOLDS)
                .len(),
            1
        );

        for _ in 0..5 {
            assert!(tracker
                .observe(&usage(83.0, Some(PERIOD)), &THRESHOLDS)
                .is_empty());
        }
    }

    #[test]
    fn the_next_threshold_up_still_fires() {
        // Having been told about 80% must not silence 95%, or the second
        // threshold would be decorative.
        let mut tracker = ThresholdTracker::new();

        tracker.observe(&usage(81.0, Some(PERIOD)), &THRESHOLDS);
        let alerts = tracker.observe(&usage(96.0, Some(PERIOD)), &THRESHOLDS);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].threshold, 95);
    }

    #[test]
    fn crossing_several_thresholds_at_once_says_only_the_highest() {
        let mut tracker = ThresholdTracker::new();

        let alerts = tracker.observe(&usage(96.0, Some(PERIOD)), &THRESHOLDS);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].threshold, 95);
    }

    #[test]
    fn a_threshold_skipped_past_is_not_announced_later() {
        // Having jumped straight to 96%, an 80% toast afterwards would be stale
        // news delivered as if it were fresh.
        let mut tracker = ThresholdTracker::new();

        tracker.observe(&usage(96.0, Some(PERIOD)), &THRESHOLDS);

        assert!(tracker
            .observe(&usage(97.0, Some(PERIOD)), &THRESHOLDS)
            .is_empty());
    }

    #[test]
    fn a_reset_time_that_drifts_between_polls_is_still_the_same_period() {
        // Observed against the real Claude endpoint: the reset timestamp comes
        // back around a second later on every poll, because it is computed
        // relative to the request. Compared exactly, every poll would look like
        // a new period, wipe the record and toast again -- every poll, for as
        // long as the quota stayed high.
        let mut tracker = ThresholdTracker::new();

        assert_eq!(
            tracker
                .observe(&usage(81.0, Some(PERIOD)), &THRESHOLDS)
                .len(),
            1
        );

        for poll in 1..=20 {
            let drifted = PERIOD + poll * 900;
            assert!(
                tracker
                    .observe(&usage(81.0, Some(drifted)), &THRESHOLDS)
                    .is_empty(),
                "drift of {}ms was mistaken for a new quota period",
                poll * 900
            );
        }
    }

    #[test]
    fn drift_does_not_accumulate_past_the_tolerance() {
        // The stored timestamp follows the newest reading, so a slow crawl
        // never adds up to a false rollover however long the app runs.
        let mut tracker = ThresholdTracker::new();
        tracker.observe(&usage(81.0, Some(PERIOD)), &THRESHOLDS);

        for poll in 1..=500 {
            let drifted = PERIOD + poll * 1_000;
            assert!(
                tracker
                    .observe(&usage(81.0, Some(drifted)), &THRESHOLDS)
                    .is_empty(),
                "accumulated drift at poll {poll} was mistaken for a new period"
            );
        }
    }

    #[test]
    fn a_new_quota_period_re_arms_every_threshold() {
        let mut tracker = ThresholdTracker::new();

        tracker.observe(&usage(96.0, Some(PERIOD)), &THRESHOLDS);

        let next_period = PERIOD + 5 * 60 * 60 * 1_000;
        let alerts = tracker.observe(&usage(85.0, Some(next_period)), &THRESHOLDS);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].threshold, 80);
    }

    #[test]
    fn a_provider_reporting_no_reset_time_re_arms_when_the_figure_drops() {
        // Some plans report a percentage and no timestamp. A fall back below
        // what was announced is the only evidence of a rollover available.
        let mut tracker = ThresholdTracker::new();

        tracker.observe(&usage(96.0, None), &THRESHOLDS);
        assert!(tracker.observe(&usage(97.0, None), &THRESHOLDS).is_empty());

        tracker.observe(&usage(4.0, None), &THRESHOLDS);
        let alerts = tracker.observe(&usage(82.0, None), &THRESHOLDS);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].threshold, 80);
    }

    #[test]
    fn the_two_windows_are_tracked_independently() {
        // They reset on different schedules, so silencing one must not silence
        // the other.
        let mut tracker = ThresholdTracker::new();

        let mut snapshot = usage(82.0, Some(PERIOD));
        snapshot.weekly = UsageWindow::new(10.0, WEEKLY_WINDOW_MINUTES, Some(PERIOD));

        assert_eq!(tracker.observe(&snapshot, &THRESHOLDS).len(), 1);

        snapshot.weekly = UsageWindow::new(96.0, WEEKLY_WINDOW_MINUTES, Some(PERIOD));
        let alerts = tracker.observe(&snapshot, &THRESHOLDS);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].window, WindowKind::Weekly);
        assert_eq!(alerts[0].threshold, 95);
    }

    #[test]
    fn no_configured_thresholds_means_silence() {
        let mut tracker = ThresholdTracker::new();

        assert!(tracker.observe(&usage(99.0, Some(PERIOD)), &[]).is_empty());
    }

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("token-drain-alerts-{name}"));
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
    fn what_has_been_announced_survives_a_restart() {
        // "Once per quota period" has to mean once per quota period, not once
        // per launch. A five-hour window with the app restarted inside it must
        // still produce one toast.
        let dir = TempDir::new("restart");

        let mut tracker = ThresholdTracker::open(&dir.0);
        assert_eq!(
            tracker
                .observe(&usage(81.0, Some(PERIOD)), &THRESHOLDS)
                .len(),
            1
        );

        let mut restarted = ThresholdTracker::open(&dir.0);

        assert!(
            restarted
                .observe(&usage(82.0, Some(PERIOD)), &THRESHOLDS)
                .is_empty(),
            "restarting the app repeated a toast inside the same quota period"
        );
    }

    #[test]
    fn a_new_period_after_a_restart_still_fires() {
        // The memory must not be so sticky that it silences the next period.
        let dir = TempDir::new("restart-new-period");

        ThresholdTracker::open(&dir.0).observe(&usage(96.0, Some(PERIOD)), &THRESHOLDS);

        let next_period = PERIOD + 5 * 60 * 60 * 1_000;
        let alerts =
            ThresholdTracker::open(&dir.0).observe(&usage(85.0, Some(next_period)), &THRESHOLDS);

        assert_eq!(alerts.len(), 1);
    }

    #[test]
    fn a_corrupt_memory_file_is_ignored_rather_than_fatal() {
        let dir = TempDir::new("corrupt");
        fs::write(dir.0.join(ALERTS_FILE_NAME), "{ not json").expect("should write");

        let mut tracker = ThresholdTracker::open(&dir.0);

        assert_eq!(
            tracker
                .observe(&usage(81.0, Some(PERIOD)), &THRESHOLDS)
                .len(),
            1
        );
    }

    #[test]
    fn the_memory_file_carries_no_credential_material() {
        let dir = TempDir::new("s1");
        let mut tracker = ThresholdTracker::open(&dir.0);
        tracker.observe(&usage(81.0, Some(PERIOD)), &THRESHOLDS);

        let contents = fs::read_to_string(dir.0.join(ALERTS_FILE_NAME)).expect("should exist");
        let json: serde_json::Value = serde_json::from_str(&contents).expect("should parse");

        let mut keys: Vec<&str> = json["records"][0]
            .as_object()
            .expect("a record object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();

        assert_eq!(
            keys,
            ["period", "provider", "thresholds", "window"],
            "the alert memory shape changed; confirm no credential material was added"
        );
    }

    #[test]
    fn the_toast_text_names_the_provider_the_window_and_the_figure() {
        let alert = Alert {
            provider: ProviderId::Codex,
            window: WindowKind::Weekly,
            threshold: 95,
            used_percent: 96.4,
        };

        assert_eq!(alert.title(), "Codex — weekly 96% used");
        assert_eq!(alert.body(), "Past the 95% mark.");
    }
}
