//! Credentials stay in Rust, are re-read each poll, and are never written here.

use std::path::{Path, PathBuf};
use reqwest::header::HeaderValue;
use serde::Deserialize;
use crate::providers::credentials::{in_home_directory, normalize_token, parse_json, read_json_file, CredentialError};

pub struct ApiCredential(pub(crate) HeaderValue);

pub struct WebCredential {
    pub(crate) cookie: HeaderValue,
    pub(crate) workspace: String,
}

impl std::fmt::Debug for WebCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WebCredential([redacted])")
    }
}

pub fn web_path() -> Result<PathBuf, CredentialError> {
    in_home_directory(&[".config", "tok-ching", "opencode.credentials.json"])
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebFile { cookie: String, workspace_id: String }

pub fn read_web() -> Result<WebCredential, CredentialError> {
    let path = web_path()?;
    web_from_file(read_json_file(&path)?, &path)
}

fn web_from_file(file: WebFile, path: &Path) -> Result<WebCredential, CredentialError> {
    let missing = |field| CredentialError::MissingField { path: path.into(), field };
    let workspace = file.workspace_id.trim();
    let suffix = workspace.strip_prefix("wrk_").or_else(|| workspace.strip_prefix("wk_"));
    if !suffix.is_some_and(|s| !s.is_empty() && s.len() <= 100 && s.bytes().all(|b| b.is_ascii_alphanumeric())) {
        return Err(missing("valid OpenCode workspace ID"));
    }
    let mut cookies = std::collections::BTreeMap::new();
    for pair in file.cookie.split(';') {
        let Some((name, value)) = pair.trim().split_once('=') else { continue };
        if !matches!(name, "auth" | "__Host-auth") { continue; }
        if value.is_empty() || !value.bytes().all(|b| (0x21..=0x7e).contains(&b) && b != b'"' && b != b'\\' && b != b',') {
            return Err(missing("valid OpenCode session cookie"));
        }
        if cookies.insert(name, value).is_some() { return Err(missing("unambiguous OpenCode session cookie")); }
    }
    if cookies.is_empty() { return Err(missing("OpenCode auth cookie")); }
    let value = cookies.iter().map(|(key,value)| format!("{key}={value}")).collect::<Vec<_>>().join("; ");
    let mut cookie = HeaderValue::from_str(&value).map_err(|_| missing("valid OpenCode session cookie"))?;
    cookie.set_sensitive(true);
    Ok(WebCredential { cookie, workspace: workspace.to_owned() })
}

impl std::fmt::Debug for ApiCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiCredential([redacted])")
    }
}

#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "type")]
    kind: String,
    key: Option<String>,
}

pub fn api_path() -> Result<PathBuf, CredentialError> {
    if let Some(data) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        let path = PathBuf::from(data);
        if path.is_absolute() { return Ok(path.join("opencode/auth.json")); }
    }
    in_home_directory(&[".local", "share", "opencode", "auth.json"])
}

pub fn read_api() -> Result<ApiCredential, CredentialError> {
    let path = api_path()?;
    if let Ok(raw) = std::env::var("OPENCODE_AUTH_CONTENT") {
        if !raw.trim().is_empty() { return parse_api(&raw, &path); }
    }
    api_from_entries(read_json_file(&path)?, &path)
}

pub fn parse_api(raw: &str, path: &Path) -> Result<ApiCredential, CredentialError> {
    api_from_entries(parse_json(raw, path)?, path)
}

fn api_from_entries(entries: std::collections::BTreeMap<String, serde_json::Value>, path: &Path) -> Result<ApiCredential, CredentialError> {
    let missing = || CredentialError::MissingField { path: path.into(), field: "OpenCode Go API key" };
    let entry: Entry = serde_json::from_value(entries.get("opencode-go").cloned().ok_or_else(missing)?)
        .map_err(|_| missing())?;
    if entry.kind != "api" { return Err(missing()); }
    let key = normalize_token(entry.key).ok_or_else(missing)?;
    if key.chars().any(char::is_whitespace) { return Err(missing()); }
    let mut header = HeaderValue::from_str(&format!("Bearer {key}")).map_err(|_| missing())?;
    header.set_sensitive(true);
    Ok(ApiCredential(header))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_auth_filters_cookies_and_rejects_injection() {
        let path = Path::new("fixture/opencode.credentials.json");
        let credential = web_from_file(WebFile { cookie: "tracking=unrelated; auth=not-a-real-session; __Host-auth=test-only".into(), workspace_id: "wrk_test123".into() }, path).unwrap();
        assert_eq!(credential.cookie.to_str().unwrap(), "__Host-auth=test-only; auth=not-a-real-session");
        assert!(credential.cookie.is_sensitive());
        assert_eq!(format!("{credential:?}"), "WebCredential([redacted])");
        for workspace in ["../other", "wrk_", "wrk_test/keys", "https://example.com"] {
            assert!(web_from_file(WebFile { cookie: "auth=test-only".into(), workspace_id: workspace.into() }, path).is_err());
        }
        for cookie in ["tracking=none", "auth=a\r\nX-Test: b", "auth=one; auth=two", "auth="] {
            assert!(web_from_file(WebFile { cookie: cookie.into(), workspace_id: "wrk_test".into() }, path).is_err());
        }
    }
    #[test]
    fn api_credential_is_validated_and_redacted() {
        let path = Path::new("fixture/auth.json");
        let credential = parse_api(r#"{"opencode-go":{"type":"api","key":"test-only-not-valid"}}"#, path).unwrap();
        assert!(credential.0.is_sensitive());
        assert!(!format!("{credential:?}").contains("test-only"));
        for raw in [r#"{}"#, r#"{"opencode-go":{"type":"oauth","key":"fake"}}"#,
            r#"{"opencode-go":{"type":"api","key":"bad\r\nvalue"}}"#, "not-json"] {
            assert!(parse_api(raw, path).is_err());
        }
    }
    #[test]
    fn malformed_secrets_never_appear_in_errors() {
        let error = parse_api("{secret-material", Path::new("fixture/auth.json")).unwrap_err();
        assert!(!format!("{error:?} {error}").contains("secret-material"));
    }

    #[test]
    fn disk_credentials_are_read_only_and_missing_files_fail() {
        let path = std::env::temp_dir().join(format!("tok-ching-go-readonly-{}.json", std::process::id()));
        let raw = r#"{"opencode-go":{"type":"api","key":"test-only-not-valid"}}"#;
        std::fs::write(&path, raw).unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        api_from_entries(read_json_file(&path).unwrap(), &path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), raw);
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), modified);
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(read_json_file::<serde_json::Value>(&path), Err(CredentialError::NotFound { .. })));
    }
}
