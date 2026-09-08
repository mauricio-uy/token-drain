//! What the user can do about a failure.
//!
//! # The read-only credential decision
//!
//! This app **never writes to a CLI's credentials file**, and does not perform
//! OAuth token refresh. When a token expires, the user reauthenticates through
//! the CLI that owns it.
//!
//! The alternative — owning the refresh and writing the rotated token back — was
//! considered and rejected. Refresh tokens are single-use and rotate on every
//! exchange, so two processes refreshing the same credentials race: whichever
//! writes second strands the other's token and the user is signed out of the
//! CLI they actually depend on. The failure mode of getting this wrong is
//! "your CLI login breaks"; the failure mode of not doing it at all is "a badge
//! says sign in again". That trade is not close.
//!
//! A consequence worth stating: because the credentials file is re-read on every
//! poll, a refresh performed by the CLI itself is picked up on the next cycle
//! with no coordination needed.
//!
//! # Why an expired token is not detected locally
//!
//! `expiresAt` is stored alongside the token and it is tempting to skip the
//! request when it has passed. This module does not, and the fetchers do not:
//! the server is authoritative, and a machine with a skewed clock would
//! otherwise refuse to poll with a perfectly valid token — a failure that looks
//! identical to a real expiry and is far harder to diagnose. One wasted request
//! that returns 401 is cheaper than that.

use serde::Serialize;

use crate::providers::credentials::CredentialError;
use crate::providers::error::UsageError;
use crate::providers::usage::ProviderId;

/// Guidance attached to a failed provider.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Remediation {
    /// One short sentence, written to be shown as-is in the hover card.
    pub message: String,
    /// The command the user should run, when running one would fix it.
    pub command: Option<&'static str>,
    /// Whether the app will recover on its own if the user does nothing.
    pub resolves_itself: bool,
}

impl Remediation {
    fn user_action(message: impl Into<String>, command: &'static str) -> Self {
        Self {
            message: message.into(),
            command: Some(command),
            resolves_itself: false,
        }
    }

    fn waiting(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            command: None,
            resolves_itself: true,
        }
    }

    fn stuck(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            command: None,
            resolves_itself: false,
        }
    }
}

impl ProviderId {
    /// The command that signs this provider's CLI in again.
    pub fn sign_in_command(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::OpencodeGo => "opencode auth login",
        }
    }

    /// Display name used in user-facing copy.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::OpencodeGo => "OpenCode Go",
        }
    }
}

/// Work out what to tell the user about a failure.
pub fn remediation_for(provider: ProviderId, error: &UsageError) -> Remediation {
    let command = provider.sign_in_command();
    let name = provider.display_name();

    match error {
        UsageError::Server { status: 403 } if provider == ProviderId::OpencodeGo =>
            Remediation::stuck("OpenCode Go requires an active subscription for this API key's user and workspace."),
        UsageError::Unauthorized => Remediation::user_action(
            format!("{name} rejected the saved login. Sign in again."),
            command,
        ),

        UsageError::MissingCredentials(credential_error) => match credential_error {
            CredentialError::NotFound { .. } | CredentialError::MissingField { .. } => {
                Remediation::user_action(format!("Not signed in to {name}."), command)
            }
            CredentialError::Malformed { .. } => Remediation::user_action(
                format!("The saved {name} login is unreadable. Sign in again."),
                command,
            ),
            // Not something a sign-in fixes: the file exists but the OS would
            // not hand it over, or there is no home directory to look in.
            CredentialError::Unreadable { .. } => {
                Remediation::stuck(format!("The saved {name} login could not be opened."))
            }
            CredentialError::NoHomeDirectory => {
                Remediation::stuck("Your home directory could not be located.".to_owned())
            }
        },

        UsageError::RateLimited { .. } => {
            Remediation::waiting(format!("{name} is rate limiting requests. Retrying later."))
        }

        UsageError::Server { status } => Remediation::waiting(format!(
            "{name} returned an error ({status}). Retrying later."
        )),

        UsageError::Network { .. } => {
            Remediation::waiting(format!("Could not reach {name}. Retrying later."))
        }

        // The signal that the undocumented contract has moved. Nothing the user
        // can do fixes it, and no amount of retrying will either.
        UsageError::Parse => Remediation::stuck(format!(
            "{name} changed its usage format. This app needs an update."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::error::NetworkFailure;
    use std::path::PathBuf;

    fn all_errors() -> Vec<UsageError> {
        vec![
            UsageError::Unauthorized,
            UsageError::Parse,
            UsageError::RateLimited {
                retry_after_ms: Some(1_000),
            },
            UsageError::Server { status: 503 },
            UsageError::Network {
                reason: NetworkFailure::Timeout,
            },
            UsageError::MissingCredentials(CredentialError::NotFound {
                path: PathBuf::from("nowhere"),
            }),
            UsageError::MissingCredentials(CredentialError::Malformed {
                path: PathBuf::from("nowhere"),
                line: 1,
                column: 2,
            }),
            UsageError::MissingCredentials(CredentialError::MissingField {
                path: PathBuf::from("nowhere"),
                field: "access token",
            }),
            UsageError::MissingCredentials(CredentialError::Unreadable {
                path: PathBuf::from("nowhere"),
                kind: std::io::ErrorKind::PermissionDenied,
            }),
            UsageError::MissingCredentials(CredentialError::NoHomeDirectory),
        ]
    }

    #[test]
    fn a_rejected_token_tells_the_user_to_sign_in_again() {
        let remediation = remediation_for(ProviderId::Claude, &UsageError::Unauthorized);

        assert_eq!(remediation.command, Some("claude"));
        assert!(!remediation.resolves_itself);
        assert!(remediation.message.contains("Sign in again"));
    }

    #[test]
    fn each_provider_names_its_own_command() {
        assert_eq!(
            remediation_for(ProviderId::Codex, &UsageError::Unauthorized).command,
            Some("codex")
        );
        assert_eq!(
            remediation_for(ProviderId::Claude, &UsageError::Unauthorized).command,
            Some("claude")
        );
    }

    #[test]
    fn transient_failures_say_they_recover_on_their_own() {
        // Why: telling the user to act when the app is already handling it wastes
        // their time and trains them to ignore the message that matters.
        for error in [
            UsageError::RateLimited {
                retry_after_ms: None,
            },
            UsageError::Server { status: 500 },
            UsageError::Network {
                reason: NetworkFailure::Connect,
            },
        ] {
            let remediation = remediation_for(ProviderId::Claude, &error);

            assert!(remediation.resolves_itself, "{error} should self-resolve");
            assert!(remediation.command.is_none(), "{error} needs no command");
        }
    }

    #[test]
    fn a_changed_contract_is_reported_as_needing_an_app_update() {
        // Not a sign-in problem, and not something retrying fixes. Suggesting
        // either would send the user chasing the wrong thing.
        let remediation = remediation_for(ProviderId::Codex, &UsageError::Parse);

        assert!(remediation.command.is_none());
        assert!(!remediation.resolves_itself);
        assert!(remediation.message.contains("needs an update"));
    }

    #[test]
    fn an_unreadable_credentials_file_does_not_suggest_signing_in() {
        // The file is there and the OS refused it. Signing in again writes the
        // same file to the same place and changes nothing.
        let remediation = remediation_for(
            ProviderId::Claude,
            &UsageError::MissingCredentials(CredentialError::Unreadable {
                path: PathBuf::from("nowhere"),
                kind: std::io::ErrorKind::PermissionDenied,
            }),
        );

        assert!(remediation.command.is_none());
        assert!(!remediation.resolves_itself);
    }

    #[test]
    fn every_error_produces_usable_copy() {
        // A sweep, so a new error variant cannot ship with an empty or
        // unpunctuated message that lands straight in the UI.
        for provider in [ProviderId::Claude, ProviderId::Codex] {
            for error in all_errors() {
                let remediation = remediation_for(provider, &error);

                assert!(
                    !remediation.message.trim().is_empty(),
                    "{provider:?}/{error} produced no message"
                );
                assert!(
                    remediation.message.ends_with('.'),
                    "{provider:?}/{error} message is not a sentence: {:?}",
                    remediation.message
                );
                assert!(
                    !(remediation.resolves_itself && remediation.command.is_some()),
                    "{provider:?}/{error} both self-resolves and demands an action"
                );
            }
        }
    }
}
