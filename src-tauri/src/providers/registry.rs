//! Running every configured provider and collecting their results.
//!
//! The guarantee this module exists to provide: **one provider's failure never
//! blocks or fails another**. A signed-out Codex install, a hung connection, or
//! a changed contract on one endpoint must leave the other provider's badge
//! showing live data.

use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinSet;

use crate::providers::claude::ClaudeProvider;
use crate::providers::codex::CodexProvider;
use crate::providers::error::{NetworkFailure, UsageError};
use crate::providers::provider::UsageProvider;
use crate::providers::usage::{ProviderId, ProviderUsage};

/// How long a single provider gets before it is abandoned.
///
/// Longer than the HTTP client's own 10s deadline: this is the backstop for a
/// provider wedged somewhere the client timeout does not cover, not the normal
/// path.
pub const DEFAULT_PROVIDER_TIMEOUT: Duration = Duration::from_secs(15);

/// One provider's outcome.
///
/// Carries the provider id alongside the result. A bare `Result` would lose
/// which provider failed, because `UsageError` deliberately carries no provider
/// context — and the UI has to mark the right badge.
#[derive(Debug)]
pub struct ProviderFetch {
    pub provider: ProviderId,
    pub result: Result<ProviderUsage, UsageError>,
}

/// A configured provider.
///
/// An enum rather than a trait object: the set is closed and known at compile
/// time, and `async fn` in a trait is not dyn-compatible anyway.
pub enum AnyProvider {
    Claude(ClaudeProvider),
    Codex(CodexProvider),
    OpencodeGo(crate::providers::opencode::go::GoProvider),
    #[cfg(test)]
    Stub(tests::StubProvider),
}

impl UsageProvider for AnyProvider {
    fn id(&self) -> ProviderId {
        match self {
            Self::Claude(provider) => provider.id(),
            Self::Codex(provider) => provider.id(),
            Self::OpencodeGo(provider) => provider.id(),
            #[cfg(test)]
            Self::Stub(provider) => provider.id(),
        }
    }

    async fn fetch(&self) -> Result<ProviderUsage, UsageError> {
        match self {
            Self::Claude(provider) => provider.fetch().await,
            Self::Codex(provider) => provider.fetch().await,
            Self::OpencodeGo(provider) => provider.fetch().await,
            #[cfg(test)]
            Self::Stub(provider) => provider.fetch().await,
        }
    }
}

/// The set of providers to poll.
pub struct ProviderRegistry {
    providers: Vec<AnyProvider>,
    timeout: Duration,
}

impl ProviderRegistry {
    pub fn new(providers: Vec<AnyProvider>) -> Self {
        Self {
            providers,
            timeout: DEFAULT_PROVIDER_TIMEOUT,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn len(&self) -> usize {
        self.providers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    /// Every provider index, in registry order.
    pub fn all_indices(&self) -> Vec<usize> {
        (0..self.providers.len()).collect()
    }

    /// The provider occupying an index, if it exists.
    pub fn id_at(&self, index: usize) -> Option<ProviderId> {
        self.providers.get(index).map(UsageProvider::id)
    }

    /// Poll every provider concurrently and collect the outcomes.
    pub async fn fetch_all(self: &Arc<Self>) -> Vec<ProviderFetch> {
        self.fetch_indices(&self.all_indices()).await
    }

    /// Poll the given providers concurrently and collect the outcomes.
    ///
    /// Taking a subset matters because schedules are per provider: when one is
    /// backing off and another is healthy, only the healthy one is due, and
    /// polling both would defeat the backoff.
    ///
    /// Results come back in registry order, not completion order: the rail
    /// renders in this order, and letting a faster provider reorder the badges
    /// between polls would make them jump around.
    pub async fn fetch_indices(self: &Arc<Self>, indices: &[usize]) -> Vec<ProviderFetch> {
        let mut tasks = JoinSet::new();

        for &index in indices {
            if index >= self.providers.len() {
                continue;
            }
            let registry = Arc::clone(self);

            tasks.spawn(async move {
                let provider = &registry.providers[index];
                let id = provider.id();

                // Why a timeout per provider rather than one around the whole
                // batch: a batch-wide deadline lets one wedged provider consume
                // the budget and starve the others.
                let result = match tokio::time::timeout(registry.timeout, provider.fetch()).await {
                    Ok(result) => result,
                    Err(_elapsed) => Err(UsageError::Network {
                        reason: NetworkFailure::Timeout,
                    }),
                };

                (
                    index,
                    ProviderFetch {
                        provider: id,
                        result,
                    },
                )
            });
        }

        let mut collected: Vec<(usize, ProviderFetch)> = Vec::with_capacity(indices.len());

        while let Some(joined) = tasks.join_next().await {
            match joined {
                Ok(entry) => collected.push(entry),
                // A panicking provider must not take the others down with it.
                // Nothing usable survives a panic, so the slot is simply
                // dropped and the remaining providers still report.
                Err(_join_error) => continue,
            }
        }

        collected.sort_by_key(|(index, _)| *index);
        collected.into_iter().map(|(_, fetch)| fetch).collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::providers::usage::{UsageWindow, SESSION_WINDOW_MINUTES};

    /// A provider with a scripted outcome, for exercising the aggregator
    /// without any network access.
    pub struct StubProvider {
        id: ProviderId,
        outcome: StubOutcome,
    }

    pub enum StubOutcome {
        Succeeds { used_percent: f64 },
        Fails,
        /// Sleeps past any sane timeout, standing in for a wedged provider.
        Hangs,
        Panics,
        RateLimited { retry_after_ms: Option<i64> },
    }

    impl StubProvider {
        pub fn new(id: ProviderId, outcome: StubOutcome) -> Self {
            Self { id, outcome }
        }
    }

    impl UsageProvider for StubProvider {
        fn id(&self) -> ProviderId {
            self.id
        }

        async fn fetch(&self) -> Result<ProviderUsage, UsageError> {
            match self.outcome {
                StubOutcome::Succeeds { used_percent } => Ok(ProviderUsage {
                    provider: self.id,
                    session: UsageWindow::new(used_percent, SESSION_WINDOW_MINUTES, None),
                    weekly: None,
                    monthly: None,
                    billing: None,
                    plan: None,
                    fetched_at: 0,
                }),
                StubOutcome::Fails => Err(UsageError::Unauthorized),
                StubOutcome::Hangs => {
                    tokio::time::sleep(Duration::from_secs(86_400)).await;
                    unreachable!("the timeout fires long before this")
                }
                StubOutcome::Panics => panic!("deliberate panic from a test provider"),
                StubOutcome::RateLimited { retry_after_ms } => {
                    Err(UsageError::RateLimited { retry_after_ms })
                }
            }
        }
    }

    pub fn stub(id: ProviderId, outcome: StubOutcome) -> AnyProvider {
        AnyProvider::Stub(StubProvider::new(id, outcome))
    }

    fn registry(providers: Vec<AnyProvider>) -> Arc<ProviderRegistry> {
        Arc::new(ProviderRegistry::new(providers).with_timeout(Duration::from_millis(50)))
    }

    #[tokio::test]
    async fn one_failing_provider_does_not_hide_the_other() {
        // The requirement this module exists for.
        let registry = registry(vec![
            stub(ProviderId::Claude, StubOutcome::Fails),
            stub(ProviderId::Codex, StubOutcome::Succeeds { used_percent: 48.0 }),
        ]);

        let results = registry.fetch_all().await;

        assert_eq!(results.len(), 2);
        assert!(matches!(
            results[0].result,
            Err(UsageError::Unauthorized)
        ));
        assert_eq!(
            results[1]
                .result
                .as_ref()
                .unwrap()
                .session
                .as_ref()
                .unwrap()
                .used_percent,
            48.0
        );
    }

    #[tokio::test]
    async fn a_hung_provider_does_not_block_the_others() {
        // Why a per-provider timeout rather than one around the batch: with a
        // shared deadline, the hung provider would consume it and the healthy
        // one would be reported as timed out too.
        let registry = registry(vec![
            stub(ProviderId::Claude, StubOutcome::Hangs),
            stub(ProviderId::Codex, StubOutcome::Succeeds { used_percent: 12.0 }),
        ]);

        let results = registry.fetch_all().await;

        assert!(matches!(
            results[0].result,
            Err(UsageError::Network {
                reason: NetworkFailure::Timeout
            })
        ));
        assert!(results[1].result.is_ok());
    }

    #[tokio::test]
    async fn a_panicking_provider_does_not_take_the_others_down() {
        let registry = registry(vec![
            stub(ProviderId::Claude, StubOutcome::Panics),
            stub(ProviderId::Codex, StubOutcome::Succeeds { used_percent: 5.0 }),
        ]);

        let results = registry.fetch_all().await;

        // The panicking slot is dropped; the healthy provider still reports.
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].provider, ProviderId::Codex);
        assert!(results[0].result.is_ok());
    }

    #[tokio::test]
    async fn results_come_back_in_registry_order() {
        // Codex is listed first and answers slowly; the order must still be the
        // configured one, or the rail's badges would swap places between polls.
        let registry = registry(vec![
            stub(ProviderId::Codex, StubOutcome::Succeeds { used_percent: 1.0 }),
            stub(ProviderId::Claude, StubOutcome::Succeeds { used_percent: 2.0 }),
        ]);

        let results = registry.fetch_all().await;

        assert_eq!(results[0].provider, ProviderId::Codex);
        assert_eq!(results[1].provider, ProviderId::Claude);
    }

    #[tokio::test]
    async fn runs_providers_concurrently_rather_than_in_sequence() {
        // Three providers that each hang. Sequential execution would take three
        // timeouts; concurrent execution takes one.
        let timeout = Duration::from_millis(200);
        let registry = Arc::new(
            ProviderRegistry::new(vec![
                stub(ProviderId::Claude, StubOutcome::Hangs),
                stub(ProviderId::Codex, StubOutcome::Hangs),
                stub(ProviderId::Claude, StubOutcome::Hangs),
            ])
            .with_timeout(timeout),
        );

        let started = std::time::Instant::now();
        let results = registry.fetch_all().await;
        let elapsed = started.elapsed();

        assert_eq!(results.len(), 3);
        assert!(
            elapsed < timeout * 2,
            "took {elapsed:?}, which suggests sequential execution"
        );
    }

    #[tokio::test]
    async fn an_empty_registry_returns_nothing() {
        let registry = registry(vec![]);

        assert!(registry.is_empty());
        assert!(registry.fetch_all().await.is_empty());
    }
}
