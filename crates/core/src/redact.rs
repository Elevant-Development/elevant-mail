//! Keeps credentials out of logs. Protocol lines go through here before they are logged.

use std::borrow::Cow;

const MASK: &str = "***";

/// Masks credentials in an IMAP command line from the client.
///
/// `a1 LOGIN user secret` becomes `a1 LOGIN *** ***`, and `a2 AUTHENTICATE XOAUTH2 <token>`
/// becomes `a2 AUTHENTICATE XOAUTH2 ***`. Other commands pass unchanged. Continuation lines of
/// an AUTHENTICATE exchange carry no command and must be dropped by the caller, not logged.
pub fn redact_imap_command(line: &str) -> Cow<'_, str> {
    let mut parts = line.splitn(3, ' ');
    let (Some(tag), Some(command)) = (parts.next(), parts.next()) else {
        return Cow::Borrowed(line);
    };
    let has_arguments = parts.next().is_some_and(|rest| !rest.trim().is_empty());

    if command.eq_ignore_ascii_case("LOGIN") && has_arguments {
        return Cow::Owned(format!("{tag} {command} {MASK} {MASK}"));
    }
    if command.eq_ignore_ascii_case("AUTHENTICATE") && has_arguments {
        let rest = line.splitn(3, ' ').nth(2).unwrap_or_default();
        let mechanism = rest.split(' ').next().unwrap_or_default();
        return Cow::Owned(if rest.trim() == mechanism {
            format!("{tag} {command} {mechanism}")
        } else {
            format!("{tag} {command} {mechanism} {MASK}")
        });
    }
    Cow::Borrowed(line)
}

/// Wrapper whose Debug and Display print a mask instead of the value.
pub struct Redacted<T>(pub T);

impl<T> std::fmt::Debug for Redacted<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(MASK)
    }
}

impl<T> std::fmt::Display for Redacted<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(MASK)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_login_credentials() {
        assert_eq!(
            redact_imap_command("a1 LOGIN alex hunter2"),
            "a1 LOGIN *** ***"
        );
        assert_eq!(
            redact_imap_command(r#"a1 login "alex@example.org" "pass with spaces""#),
            "a1 login *** ***"
        );
    }

    #[test]
    fn masks_authenticate_payload_but_keeps_mechanism() {
        assert_eq!(
            redact_imap_command("a2 AUTHENTICATE XOAUTH2 dXNlcj1hbGV4AWF1dGg9QmVhcmVy"),
            "a2 AUTHENTICATE XOAUTH2 ***"
        );
        assert_eq!(
            redact_imap_command("a2 AUTHENTICATE PLAIN"),
            "a2 AUTHENTICATE PLAIN"
        );
    }

    #[test]
    fn leaves_other_commands_alone() {
        let line = "a3 SELECT INBOX";
        assert!(matches!(redact_imap_command(line), Cow::Borrowed(_)));
        assert_eq!(redact_imap_command("a4 CAPABILITY"), "a4 CAPABILITY");
        assert_eq!(redact_imap_command(""), "");
    }

    #[test]
    fn redacted_hides_value() {
        let secret = Redacted("hunter2");
        assert_eq!(format!("{secret} {secret:?}"), "*** ***");
    }
}
