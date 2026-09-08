//! Zen is prepaid workspace billing, not a three-window subscription.
use reqwest::Client;
use super::{billing::parse_billing, credentials, read_response};
use crate::providers::error::UsageError;
use crate::providers::http::classify_transport_error;
use crate::providers::provider::UsageProvider;
use crate::providers::usage::{ProviderId, ProviderUsage};

pub struct ZenProvider { client: Client }
impl ZenProvider { pub fn new(client: Client) -> Self { Self { client } } }

impl UsageProvider for ZenProvider {
    fn id(&self) -> ProviderId { ProviderId::OpencodeZen }
    async fn fetch(&self) -> Result<ProviderUsage, UsageError> {
        let credential = credentials::read_web()?;
        let response = self.client.get(format!("https://opencode.ai/workspace/{}/billing", credential.workspace))
            .header(reqwest::header::COOKIE, credential.cookie)
            .header(reqwest::header::ACCEPT, "text/html")
            .send().await.map_err(classify_transport_error)?;
        let body = read_response(response).await?;
        let text = std::str::from_utf8(&body).map_err(|_| UsageError::Parse)?;
        Ok(ProviderUsage {
            provider: self.id(), session: None, weekly: None, monthly: None,
            billing: Some(parse_billing(text)?), plan: Some("Pay as you go".into()),
            fetched_at: chrono::Utc::now().timestamp_millis(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "requires an explicitly configured OpenCode web session; reads billing only"]
    async fn live_zen_billing() {
        let provider = ZenProvider::new(super::super::build_client().unwrap());
        let usage = provider.fetch().await.expect("Zen billing request failed");
        assert!(usage.billing.is_some());
        assert!(usage.session.is_none() && usage.weekly.is_none() && usage.monthly.is_none());
    }
}
