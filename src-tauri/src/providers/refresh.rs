//! The background polling loop.
//!
//! Drives the per-provider [`RefreshSchedule`]s: works out which providers are
//! due, polls exactly those, feeds each outcome back into its own schedule, and
//! publishes the batch.
//!
//! Schedules are per provider, not global. A provider that is rate limited or
//! backing off must not delay a healthy one, and a healthy one must not drag a
//! backing-off provider back into a fast poll.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, Notify};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::providers::registry::{ProviderFetch, ProviderRegistry};
use crate::providers::schedule::{RefreshSchedule, DEFAULT_POLL_INTERVAL, MIN_POLL_INTERVAL};
use crate::providers::usage::ProviderId;

/// What the loop should be doing right now.
///
/// Consulted on every pass rather than captured at startup, which is what lets a
/// changed poll interval or a provider being switched off take effect without a
/// restart. A trait rather than a concrete settings type keeps this module — and
/// everything below it — unaware that the app has a settings screen at all.
pub trait RefreshPolicy: Send + Sync + 'static {
    /// Gap between polls of a healthy provider.
    fn interval(&self) -> Duration;

    /// Whether this provider should be polled at all.
    fn is_enabled(&self, provider: ProviderId) -> bool;
}

/// Tuning for the polling loop.
#[derive(Debug, Clone)]
pub struct RefreshConfig {
    /// Gap between polls of a healthy provider. Clamped up to the hard floor by
    /// [`RefreshSchedule`].
    pub interval: Duration,
}

impl Default for RefreshConfig {
    fn default() -> Self {
        Self {
            interval: DEFAULT_POLL_INTERVAL,
        }
    }
}

/// A fixed configuration: one interval, every provider on.
impl RefreshPolicy for RefreshConfig {
    fn interval(&self) -> Duration {
        self.interval
    }

    fn is_enabled(&self, _provider: ProviderId) -> bool {
        true
    }
}

/// State the loop keeps for one provider.
struct ProviderState {
    schedule: RefreshSchedule,
    due_at: Instant,
    last_polled: Option<Instant>,
}

/// Bring every provider forward to the earliest moment the floor allows.
///
/// A manual refresh may skip a remaining interval or a backoff, but it may not
/// breach the minimum poll interval: a user holding down a refresh button must
/// not be able to do what the configuration itself is forbidden from doing.
fn bring_forward(states: &mut [ProviderState], now: Instant) {
    for state in states {
        let earliest = match state.last_polled {
            Some(last) => last + MIN_POLL_INTERVAL,
            None => now,
        };
        state.due_at = state.due_at.min(earliest.max(now)).max(earliest);
    }
}

/// Run the polling loop until the receiver is dropped.
///
/// Every provider is due immediately on the first pass, so the widget shows
/// live data as soon as it starts rather than after one interval of nothing.
pub async fn run_refresh_loop(
    registry: Arc<ProviderRegistry>,
    policy: Arc<dyn RefreshPolicy>,
    sink: mpsc::Sender<Vec<ProviderFetch>>,
    refresh_now: Arc<Notify>,
) {
    if registry.is_empty() {
        return;
    }

    let now = Instant::now();
    let mut states: Vec<ProviderState> = (0..registry.len())
        .map(|_| ProviderState {
            schedule: RefreshSchedule::new(policy.interval()),
            due_at: now,
            last_polled: None,
        })
        .collect();

    loop {
        let now = Instant::now();

        // Re-read the interval every pass. Backoff state is left alone, so a
        // provider that has been failing keeps its escalation, now measured
        // against the new interval.
        let interval = policy.interval();
        for state in &mut states {
            state.schedule.set_interval(interval);
        }

        let enabled = |index: usize| {
            registry
                .id_at(index)
                .map(|provider| policy.is_enabled(provider))
                .unwrap_or(true)
        };

        let due: Vec<usize> = states
            .iter()
            .enumerate()
            .filter(|(index, state)| enabled(*index) && state.due_at <= now)
            .map(|(index, _)| index)
            .collect();

        if due.is_empty() {
            // Only enabled providers have deadlines worth waiting for. A
            // disabled one would hand back a deadline already in the past and
            // the loop would spin on it.
            let next = states
                .iter()
                .enumerate()
                .filter(|(index, _)| enabled(*index))
                .map(|(_, state)| state.due_at)
                .min();

            match next {
                // Sleep until the earliest deadline rather than waking on a
                // fixed tick: a loop that wakes every second to discover
                // nothing is due keeps the process from ever going idle.
                Some(next) => {
                    tokio::select! {
                        _ = tokio::time::sleep_until(next) => {}
                        _ = refresh_now.notified() => bring_forward(&mut states, Instant::now()),
                    }
                }
                // Every provider is switched off. There is no deadline left to
                // wait for, so wait for the configuration to change instead.
                None => {
                    refresh_now.notified().await;
                    bring_forward(&mut states, Instant::now());
                }
            }
            continue;
        }

        let results = registry.fetch_indices(&due).await;

        // Feed each outcome back into its own schedule. `fetch_indices` returns
        // results in registry order, and `due` is built in that same order, so
        // the two line up.
        for (position, index) in due.iter().enumerate() {
            let Some(fetch) = results.get(position) else {
                // A provider whose task panicked produces no result. Give it
                // the same treatment as a failure so it is retried later rather
                // than being polled every pass forever.
                if let Some(state) = states.get_mut(*index) {
                    state.last_polled = Some(Instant::now());
                    state.due_at = Instant::now() + state.schedule.interval();
                }
                continue;
            };

            if let Some(state) = states.get_mut(*index) {
                let delay = match &fetch.result {
                    Ok(_) => state.schedule.record_success(),
                    Err(error) => state.schedule.record_failure(error),
                };
                state.last_polled = Some(Instant::now());
                state.due_at = Instant::now() + delay;
            }
        }

        // A closed channel means the consumer is gone, so there is nobody left
        // to poll for.
        if sink.send(results).await.is_err() {
            return;
        }
    }
}

/// Spawn [`run_refresh_loop`] as a background task.
pub fn spawn_refresh_loop(
    registry: Arc<ProviderRegistry>,
    policy: Arc<dyn RefreshPolicy>,
    sink: mpsc::Sender<Vec<ProviderFetch>>,
    refresh_now: Arc<Notify>,
) -> JoinHandle<()> {
    tokio::spawn(run_refresh_loop(registry, policy, sink, refresh_now))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::error::UsageError;
    use crate::providers::registry::tests::{stub, StubOutcome};
    use crate::providers::schedule::{MAX_BACKOFF, MIN_POLL_INTERVAL};
    use crate::providers::usage::ProviderId;

    const INTERVAL: Duration = Duration::from_secs(300);

    fn config() -> RefreshConfig {
        RefreshConfig { interval: INTERVAL }
    }

    /// Collect one batch, failing the test rather than hanging if none arrives.
    ///
    /// The deadline is deliberately far longer than any delay under test. With
    /// a paused clock the runtime auto-advances to the earliest pending
    /// deadline, so a short safety net here would race the very timers being
    /// asserted and fire first. Being long costs nothing: if no batch is coming,
    /// the clock jumps straight to this deadline and the test fails at once.
    const RECEIVE_DEADLINE: Duration = Duration::from_secs(3_600);

    async fn next_batch(receiver: &mut mpsc::Receiver<Vec<ProviderFetch>>) -> Vec<ProviderFetch> {
        tokio::time::timeout(RECEIVE_DEADLINE, receiver.recv())
            .await
            .expect("a batch should have been published")
            .expect("the loop should still be running")
    }

    fn providers_in(batch: &[ProviderFetch]) -> Vec<ProviderId> {
        batch.iter().map(|fetch| fetch.provider).collect()
    }

    #[tokio::test(start_paused = true)]
    async fn polls_every_provider_immediately_on_startup() {
        // Why: otherwise the widget shows nothing for the first interval after
        // launch, which reads as broken.
        let registry = Arc::new(ProviderRegistry::new(vec![
            stub(ProviderId::Claude, StubOutcome::Succeeds { used_percent: 1.0 }),
            stub(ProviderId::Codex, StubOutcome::Succeeds { used_percent: 2.0 }),
        ]));
        let (sender, mut receiver) = mpsc::channel(8);
        let _task = spawn_refresh_loop(registry, Arc::new(config()), sender, Arc::new(Notify::new()));

        let batch = next_batch(&mut receiver).await;

        assert_eq!(
            providers_in(&batch),
            vec![ProviderId::Claude, ProviderId::Codex]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_healthy_provider_is_repolled_at_the_interval() {
        let registry = Arc::new(ProviderRegistry::new(vec![stub(
            ProviderId::Claude,
            StubOutcome::Succeeds { used_percent: 1.0 },
        )]));
        let (sender, mut receiver) = mpsc::channel(8);
        let _task = spawn_refresh_loop(registry, Arc::new(config()), sender, Arc::new(Notify::new()));

        next_batch(&mut receiver).await;

        // Just short of the interval: nothing yet.
        tokio::time::advance(INTERVAL - Duration::from_secs(1)).await;
        assert!(receiver.try_recv().is_err(), "polled before it was due");

        tokio::time::advance(Duration::from_secs(2)).await;
        assert_eq!(providers_in(&next_batch(&mut receiver).await), vec![ProviderId::Claude]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_failing_provider_backs_off_while_a_healthy_one_keeps_polling() {
        // The reason schedules are per provider. A shared schedule would either
        // hold the healthy provider back or drag the failing one forward.
        let registry = Arc::new(ProviderRegistry::new(vec![
            stub(ProviderId::Claude, StubOutcome::Fails),
            stub(ProviderId::Codex, StubOutcome::Succeeds { used_percent: 2.0 }),
        ]));
        let (sender, mut receiver) = mpsc::channel(8);
        let _task = spawn_refresh_loop(registry, Arc::new(config()), sender, Arc::new(Notify::new()));

        assert_eq!(next_batch(&mut receiver).await.len(), 2);

        // Claude failed with Unauthorized, a permanent error, so it is parked
        // at the ceiling. Codex is due again at the interval, alone.
        tokio::time::advance(INTERVAL).await;
        assert_eq!(
            providers_in(&next_batch(&mut receiver).await),
            vec![ProviderId::Codex]
        );

        tokio::time::advance(INTERVAL).await;
        assert_eq!(
            providers_in(&next_batch(&mut receiver).await),
            vec![ProviderId::Codex]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_transient_failure_doubles_the_delay() {
        // A short provider timeout so the hang resolves quickly in test time.
        let registry = Arc::new(
            ProviderRegistry::new(vec![stub(ProviderId::Claude, StubOutcome::Hangs)])
                .with_timeout(Duration::from_secs(1)),
        );
        let (sender, mut receiver) = mpsc::channel(8);
        let _task = spawn_refresh_loop(registry, Arc::new(config()), sender, Arc::new(Notify::new()));

        let batch = next_batch(&mut receiver).await;
        assert!(matches!(batch[0].result, Err(UsageError::Network { .. })));

        // A timeout is transient, so the next attempt is at twice the interval,
        // not at the interval.
        tokio::time::advance(INTERVAL).await;
        assert!(receiver.try_recv().is_err(), "backoff was not applied");

        tokio::time::advance(INTERVAL).await;
        assert_eq!(next_batch(&mut receiver).await.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_short_retry_after_is_still_floored() {
        // The server asks us back in one second; the floor says one minute.
        let registry = Arc::new(ProviderRegistry::new(vec![stub(
            ProviderId::Claude,
            StubOutcome::RateLimited {
                retry_after_ms: Some(1_000),
            },
        )]));
        let (sender, mut receiver) = mpsc::channel(8);
        let _task = spawn_refresh_loop(registry, Arc::new(config()), sender, Arc::new(Notify::new()));

        next_batch(&mut receiver).await;

        tokio::time::advance(MIN_POLL_INTERVAL - Duration::from_secs(2)).await;
        assert!(
            receiver.try_recv().is_err(),
            "polled faster than the floor allows"
        );

        tokio::time::advance(Duration::from_secs(4)).await;
        assert_eq!(next_batch(&mut receiver).await.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_long_retry_after_is_respected() {
        let registry = Arc::new(ProviderRegistry::new(vec![stub(
            ProviderId::Claude,
            StubOutcome::RateLimited {
                retry_after_ms: Some(900_000),
            },
        )]));
        let (sender, mut receiver) = mpsc::channel(8);
        let _task = spawn_refresh_loop(registry, Arc::new(config()), sender, Arc::new(Notify::new()));

        next_batch(&mut receiver).await;

        tokio::time::advance(Duration::from_secs(890)).await;
        assert!(receiver.try_recv().is_err(), "ignored the Retry-After delay");

        tokio::time::advance(Duration::from_secs(20)).await;
        assert_eq!(next_batch(&mut receiver).await.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_permanent_failure_is_parked_at_the_ceiling() {
        let registry = Arc::new(ProviderRegistry::new(vec![stub(
            ProviderId::Claude,
            StubOutcome::Fails,
        )]));
        let (sender, mut receiver) = mpsc::channel(8);
        let _task = spawn_refresh_loop(registry, Arc::new(config()), sender, Arc::new(Notify::new()));

        next_batch(&mut receiver).await;

        tokio::time::advance(MAX_BACKOFF - Duration::from_secs(5)).await;
        assert!(receiver.try_recv().is_err(), "retried a permanent failure too soon");

        // But it is still retried eventually, so recovery after the user signs
        // back in is automatic.
        tokio::time::advance(Duration::from_secs(10)).await;
        assert_eq!(next_batch(&mut receiver).await.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_manual_refresh_skips_the_remaining_interval() {
        let registry = Arc::new(ProviderRegistry::new(vec![stub(
            ProviderId::Claude,
            StubOutcome::Succeeds { used_percent: 1.0 },
        )]));
        let (sender, mut receiver) = mpsc::channel(8);
        let refresh = Arc::new(Notify::new());
        let _task = spawn_refresh_loop(registry, Arc::new(config()), sender, Arc::clone(&refresh));

        next_batch(&mut receiver).await;

        // Past the floor but well short of the interval.
        tokio::time::advance(MIN_POLL_INTERVAL + Duration::from_secs(5)).await;
        assert!(receiver.try_recv().is_err(), "polled without being asked");

        refresh.notify_waiters();
        assert_eq!(next_batch(&mut receiver).await.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_manual_refresh_cannot_breach_the_floor() {
        // A user holding down a refresh button must not be able to do what the
        // configuration itself is forbidden from doing.
        let registry = Arc::new(ProviderRegistry::new(vec![stub(
            ProviderId::Claude,
            StubOutcome::Succeeds { used_percent: 1.0 },
        )]));
        let (sender, mut receiver) = mpsc::channel(8);
        let refresh = Arc::new(Notify::new());
        let _task = spawn_refresh_loop(registry, Arc::new(config()), sender, Arc::clone(&refresh));

        next_batch(&mut receiver).await;

        tokio::time::advance(Duration::from_secs(5)).await;
        refresh.notify_waiters();
        tokio::time::advance(Duration::from_secs(5)).await;
        assert!(
            receiver.try_recv().is_err(),
            "a manual refresh polled inside the floor"
        );

        // Once the floor has passed, the brought-forward poll happens.
        tokio::time::advance(MIN_POLL_INTERVAL).await;
        assert_eq!(next_batch(&mut receiver).await.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_manual_refresh_revives_a_provider_parked_at_the_ceiling() {
        // The point of the button: a provider that failed permanently should not
        // make the user wait out the ceiling after they have fixed it.
        let registry = Arc::new(ProviderRegistry::new(vec![stub(
            ProviderId::Claude,
            StubOutcome::Fails,
        )]));
        let (sender, mut receiver) = mpsc::channel(8);
        let refresh = Arc::new(Notify::new());
        let _task = spawn_refresh_loop(registry, Arc::new(config()), sender, Arc::clone(&refresh));

        next_batch(&mut receiver).await;

        tokio::time::advance(MIN_POLL_INTERVAL + Duration::from_secs(1)).await;
        refresh.notify_waiters();

        assert_eq!(next_batch(&mut receiver).await.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn the_loop_stops_when_the_consumer_goes_away() {
        let registry = Arc::new(ProviderRegistry::new(vec![stub(
            ProviderId::Claude,
            StubOutcome::Succeeds { used_percent: 1.0 },
        )]));
        let (sender, receiver) = mpsc::channel(8);
        let task = spawn_refresh_loop(registry, Arc::new(config()), sender, Arc::new(Notify::new()));

        drop(receiver);
        tokio::time::advance(INTERVAL * 2).await;

        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("the loop should have exited")
            .expect("the loop should not have panicked");
    }

    #[tokio::test(start_paused = true)]
    async fn an_empty_registry_exits_immediately() {
        let registry = Arc::new(ProviderRegistry::new(vec![]));
        let (sender, _receiver) = mpsc::channel(8);

        let task = spawn_refresh_loop(registry, Arc::new(config()), sender, Arc::new(Notify::new()));

        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("the loop should have exited")
            .expect("the loop should not have panicked");
    }

    /// A policy that can be changed while the loop is running, standing in for
    /// the user editing their settings.
    struct MutablePolicy {
        interval: std::sync::Mutex<Duration>,
        disabled: std::sync::Mutex<Vec<ProviderId>>,
    }

    impl MutablePolicy {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                interval: std::sync::Mutex::new(INTERVAL),
                disabled: std::sync::Mutex::new(Vec::new()),
            })
        }

        fn disable(&self, provider: ProviderId) {
            self.disabled.lock().unwrap().push(provider);
        }

        fn enable_everything(&self) {
            self.disabled.lock().unwrap().clear();
        }

        fn set_interval(&self, interval: Duration) {
            *self.interval.lock().unwrap() = interval;
        }
    }

    impl RefreshPolicy for MutablePolicy {
        fn interval(&self) -> Duration {
            *self.interval.lock().unwrap()
        }

        fn is_enabled(&self, provider: ProviderId) -> bool {
            !self.disabled.lock().unwrap().contains(&provider)
        }
    }

    fn two_healthy_providers() -> Arc<ProviderRegistry> {
        Arc::new(ProviderRegistry::new(vec![
            stub(ProviderId::Claude, StubOutcome::Succeeds { used_percent: 1.0 }),
            stub(ProviderId::Codex, StubOutcome::Succeeds { used_percent: 2.0 }),
        ]))
    }

    #[tokio::test(start_paused = true)]
    async fn a_disabled_provider_is_never_polled() {
        // Switching a provider off has to stop the requests, not just hide the
        // badge. Otherwise the app keeps authenticating against a service the
        // user has said they are not using.
        let policy = MutablePolicy::new();
        policy.disable(ProviderId::Codex);

        let (sender, mut receiver) = mpsc::channel(8);
        let _task = spawn_refresh_loop(
            two_healthy_providers(),
            Arc::clone(&policy) as Arc<dyn RefreshPolicy>,
            sender,
            Arc::new(Notify::new()),
        );

        assert_eq!(
            providers_in(&next_batch(&mut receiver).await),
            vec![ProviderId::Claude]
        );

        tokio::time::advance(INTERVAL * 3).await;

        assert_eq!(
            providers_in(&next_batch(&mut receiver).await),
            vec![ProviderId::Claude],
            "a disabled provider was polled anyway"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn re_enabling_a_provider_polls_it_without_a_restart() {
        let policy = MutablePolicy::new();
        policy.disable(ProviderId::Codex);

        let refresh = Arc::new(Notify::new());
        let (sender, mut receiver) = mpsc::channel(8);
        let _task = spawn_refresh_loop(
            two_healthy_providers(),
            Arc::clone(&policy) as Arc<dyn RefreshPolicy>,
            sender,
            Arc::clone(&refresh),
        );

        next_batch(&mut receiver).await;

        // Past the floor, so the nudge is allowed to act at once.
        tokio::time::advance(MIN_POLL_INTERVAL + Duration::from_secs(1)).await;
        policy.enable_everything();
        refresh.notify_waiters();

        let batch = next_batch(&mut receiver).await;

        assert!(
            batch.iter().any(|fetch| fetch.provider == ProviderId::Codex),
            "a re-enabled provider was not picked up until a restart"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_shortened_interval_takes_effect_without_a_restart() {
        // The schedules are built once at startup, so this only passes because
        // the loop re-reads the interval on every pass.
        let policy = MutablePolicy::new();

        let (sender, mut receiver) = mpsc::channel(8);
        let _task = spawn_refresh_loop(
            Arc::new(ProviderRegistry::new(vec![stub(
                ProviderId::Claude,
                StubOutcome::Succeeds { used_percent: 1.0 },
            )])),
            Arc::clone(&policy) as Arc<dyn RefreshPolicy>,
            sender,
            Arc::new(Notify::new()),
        );

        next_batch(&mut receiver).await;
        policy.set_interval(MIN_POLL_INTERVAL);

        // Well inside the original 300s interval, well past the new 60s one.
        tokio::time::advance(MIN_POLL_INTERVAL + Duration::from_secs(5)).await;

        assert_eq!(
            providers_in(&next_batch(&mut receiver).await),
            vec![ProviderId::Claude],
            "the loop was still using the interval it started with"
        );
    }
}
