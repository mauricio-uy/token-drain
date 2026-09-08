//! Read-only OpenCode subscription and workspace billing integrations.

pub mod credentials;
pub mod go;
pub mod billing;
pub mod zen;

use std::time::Duration;
use reqwest::Client;
use crate::providers::error::{NetworkFailure, UsageError};
use crate::providers::http::{classify_status, classify_transport_error, retry_after_header};

pub fn build_client() -> Result<Client, UsageError> {
    Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("tok-ching/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| UsageError::Network { reason: NetworkFailure::Request })
}

/// Bound response memory and discard all transport detail before returning errors.
pub(crate) async fn read_response(mut response: reqwest::Response) -> Result<Vec<u8>, UsageError> {
    let status = response.status().as_u16();
    if (300..400).contains(&status) { return Err(UsageError::Unauthorized); }
    if let Some(error) = classify_status(status, retry_after_header(&response).as_deref(), chrono::Utc::now().timestamp_millis()) {
        return Err(error);
    }
    const MAX_BODY: usize = 2_000_000;
    if response.content_length().is_some_and(|size| size > MAX_BODY as u64) {
        return Err(UsageError::Parse);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(classify_transport_error)? {
        if body.len().saturating_add(chunk.len()) > MAX_BODY { return Err(UsageError::Parse); }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[tokio::test]
    async fn http_failures_and_redirects_are_bounded_and_classified() {
        for (status, length) in [(401, 0), (403, 0), (429, 0), (500, 0), (302, 0), (200, 2_000_001)] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
                let mut buffer = [0; 2048];
                socket.read(&mut buffer).unwrap();
                write!(socket, "HTTP/1.1 {status} Test\r\nContent-Length: {length}\r\nRetry-After: 60\r\nLocation: http://127.0.0.1:1/not-followed\r\nConnection: close\r\n\r\n").unwrap();
            });
            let response = build_client().unwrap().get(url).send().await.unwrap();
            let error = read_response(response).await.unwrap_err();
            server.join().unwrap();
            match status {
                401 | 302 => assert!(matches!(error, UsageError::Unauthorized)),
                429 => assert!(matches!(error, UsageError::RateLimited { retry_after_ms: Some(60000) })),
                200 => assert!(matches!(error, UsageError::Parse)),
                _ => assert!(matches!(error, UsageError::Server { status: code } if code == status)),
            }
        }
    }
}
