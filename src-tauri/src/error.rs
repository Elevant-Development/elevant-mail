//! What a failed command hands to the frontend: a kind and a hint, both translated there.
//! Details go to the log only, so nothing internal or secret reaches the UI.

use elevant_mail_core::{Error, ErrorKind, Hint};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    kind: ErrorKind,
    hint: Option<Hint>,
}

impl From<Error> for CommandError {
    fn from(error: Error) -> Self {
        tracing::error!(error = %error_chain(&error), "command failed");
        Self {
            kind: error.kind(),
            hint: error.hint(),
        }
    }
}

/// "outer: inner: root cause", for one readable log line.
fn error_chain(error: &dyn std::error::Error) -> String {
    let mut chain = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        chain.push_str(": ");
        chain.push_str(&cause.to_string());
        source = cause.source();
    }
    chain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_kind_and_hint_only() {
        let error = CommandError::from(Error::SecretNotFound);
        assert_eq!(
            serde_json::to_string(&error).unwrap(),
            r#"{"kind":"credentialMissing","hint":"enterPasswordAgain"}"#
        );
    }

    #[test]
    fn chain_includes_the_cause() {
        let io = std::io::Error::other("disk full");
        assert_eq!(error_chain(&Error::Io(io)), "file system error: disk full");
    }
}
