//! Shared plumbing for reading a CLI's credentials file from disk.
//!
//! Every provider stores its OAuth material as JSON somewhere under the user's
//! home directory. The file location and the field names differ; the failure
//! modes and the rules for handling them do not, so they live here.
//!
//! Security: nothing in this module ever renders file contents. A syntax error
//! is reported by position only, so a corrupt credentials file cannot spill
//! token material into a log.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

/// Every distinct way reading a credentials file can fail.
///
/// The variants are deliberately fine-grained: the UI renders a different badge
/// state for "you never signed in" than for "the file is corrupt", and
/// collapsing them would make both look like the same problem.
#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("could not determine the current user's home directory")]
    NoHomeDirectory,

    #[error("no credentials file at {path}")]
    NotFound { path: PathBuf },

    #[error("credentials file at {path} could not be read ({kind:?})")]
    Unreadable { path: PathBuf, kind: io::ErrorKind },

    /// Only the position of the syntax error is carried, never the surrounding
    /// text.
    #[error("credentials file at {path} is not valid JSON (line {line}, column {column})")]
    Malformed {
        path: PathBuf,
        line: usize,
        column: usize,
    },

    /// A structurally valid file that does not carry what we need. `field`
    /// names the missing piece so the message stays actionable.
    #[error("credentials file at {path} has no {field}")]
    MissingField {
        path: PathBuf,
        field: &'static str,
    },
}

/// Read and deserialize a credentials file.
pub fn read_json_file<T: DeserializeOwned>(path: &Path) -> Result<T, CredentialError> {
    let raw = fs::read_to_string(path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => CredentialError::NotFound {
            path: path.to_path_buf(),
        },
        kind => CredentialError::Unreadable {
            path: path.to_path_buf(),
            kind,
        },
    })?;

    parse_json(&raw, path)
}

/// Deserialize credentials JSON. Pure, so parsing rules are testable without
/// touching the filesystem. `path` is used only to build error messages.
pub fn parse_json<T: DeserializeOwned>(raw: &str, path: &Path) -> Result<T, CredentialError> {
    serde_json::from_str(raw).map_err(|error| CredentialError::Malformed {
        path: path.to_path_buf(),
        line: error.line(),
        column: error.column(),
    })
}

/// Normalize an optional token field: trim it, and treat blank as absent.
///
/// A whitespace-only token would otherwise turn a clear "not signed in" into a
/// confusing 401 from the server.
pub fn normalize_token(value: Option<String>) -> Option<String> {
    value
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
}

/// Resolve a path inside the user's home directory.
pub fn in_home_directory(segments: &[&str]) -> Result<PathBuf, CredentialError> {
    let mut path = dirs::home_dir().ok_or(CredentialError::NoHomeDirectory)?;
    for segment in segments {
        path.push(segment);
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct Sample {
        value: Option<String>,
    }

    fn fixture_path() -> PathBuf {
        PathBuf::from("test-fixture/credentials.json")
    }

    #[test]
    fn deserializes_a_well_formed_file() {
        let parsed: Sample =
            parse_json(r#"{"value": "present"}"#, &fixture_path()).expect("should parse");

        assert_eq!(parsed.value.as_deref(), Some("present"));
    }

    #[test]
    fn reports_a_missing_file() {
        let path = std::env::temp_dir().join("token-drain-shared-absent-fixture.json");
        assert!(!path.exists(), "test precondition");

        assert!(matches!(
            read_json_file::<Sample>(&path),
            Err(CredentialError::NotFound { .. })
        ));
    }

    #[test]
    fn reports_an_unreadable_file() {
        // A directory in place of a file produces a non-NotFound io error,
        // which is the branch under test and is reproducible everywhere.
        let dir = std::env::temp_dir().join("token-drain-shared-unreadable-fixture");
        fs::create_dir_all(&dir).expect("should create fixture directory");

        let result = read_json_file::<Sample>(&dir);

        fs::remove_dir_all(&dir).ok();

        assert!(matches!(result, Err(CredentialError::Unreadable { .. })));
    }

    #[test]
    fn reports_malformed_json_without_echoing_content() {
        let raw = r#"{"value": "secret-material"#;

        let error = parse_json::<Sample>(raw, &fixture_path()).expect_err("should fail");

        assert!(matches!(error, CredentialError::Malformed { .. }));
        assert!(!error.to_string().contains("secret-material"));
    }

    #[test]
    fn normalizes_blank_tokens_to_absent() {
        assert_eq!(normalize_token(Some("  abc  ".into())), Some("abc".into()));
        assert_eq!(normalize_token(Some("   ".into())), None);
        assert_eq!(normalize_token(Some(String::new())), None);
        assert_eq!(normalize_token(None), None);
    }
}
