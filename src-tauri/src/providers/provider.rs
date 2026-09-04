//! The interface every usage provider implements.
//!
//! A provider owns the whole path from "credentials on disk" to "a
//! [`ProviderUsage`] snapshot": which file to read, which endpoint to call, and
//! how to translate the payload. Everything above this layer knows only the
//! trait.

use std::future::Future;

use crate::providers::error::UsageError;
use crate::providers::usage::{ProviderId, ProviderUsage};

/// One source of usage data.
///
/// `fetch` returns `impl Future + Send` rather than being a bare `async fn` so
/// implementations stay usable from a concurrent runner: a plain `async fn` in a
/// trait produces a future with no `Send` bound, which cannot be scheduled
/// across worker threads.
pub trait UsageProvider {
    fn id(&self) -> ProviderId;

    fn fetch(&self) -> impl Future<Output = Result<ProviderUsage, UsageError>> + Send;
}
