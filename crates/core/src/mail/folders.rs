//! Folders as the server lists them, with roles and readable names.

use async_imap::types::{Name, NameAttribute};

use crate::model::FolderRole;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFolder {
    /// Server path, modified UTF-7 as IMAP sends it; used for all commands.
    pub path: String,
    /// Last path segment, decoded for display.
    pub name: String,
    pub delimiter: Option<String>,
    pub role: Option<FolderRole>,
    /// `\Noselect` folders only group others and hold no mail.
    pub selectable: bool,
}

impl RemoteFolder {
    pub fn from_name(name: &Name) -> Self {
        let attributes = name.attributes();
        Self::new(
            name.name(),
            name.delimiter(),
            role_from_attributes(attributes),
            !attributes
                .iter()
                .any(|attribute| matches!(attribute, NameAttribute::NoSelect)),
        )
    }

    pub fn new(
        path: &str,
        delimiter: Option<&str>,
        role: Option<FolderRole>,
        selectable: bool,
    ) -> Self {
        let last_segment = match delimiter {
            Some(delimiter) if !delimiter.is_empty() => {
                path.rsplit(delimiter).next().unwrap_or(path)
            }
            _ => path,
        };
        let name = utf7_imap::decode_utf7_imap(last_segment.to_owned());
        let role = role.or_else(|| role_from_name(path, &name));
        Self {
            path: path.to_owned(),
            name,
            delimiter: delimiter.map(str::to_owned),
            role,
            selectable,
        }
    }
}

fn role_from_attributes(attributes: &[NameAttribute<'_>]) -> Option<FolderRole> {
    attributes.iter().find_map(|attribute| match attribute {
        NameAttribute::Sent => Some(FolderRole::Sent),
        NameAttribute::Drafts => Some(FolderRole::Drafts),
        NameAttribute::Trash => Some(FolderRole::Trash),
        NameAttribute::Junk => Some(FolderRole::Junk),
        NameAttribute::Archive => Some(FolderRole::Archive),
        NameAttribute::All => Some(FolderRole::All),
        _ => None,
    })
}

/// Fallback for servers without SPECIAL-USE: common English and German names.
fn role_from_name(path: &str, name: &str) -> Option<FolderRole> {
    if path.eq_ignore_ascii_case("INBOX") {
        return Some(FolderRole::Inbox);
    }
    let name = name.to_lowercase();
    let matches = |candidates: &[&str]| candidates.contains(&name.as_str());
    if matches(&[
        "sent",
        "sent items",
        "sent messages",
        "sent mail",
        "gesendet",
        "gesendete elemente",
        "gesendete objekte",
    ]) {
        Some(FolderRole::Sent)
    } else if matches(&["drafts", "draft", "entwürfe", "entwurf"]) {
        Some(FolderRole::Drafts)
    } else if matches(&[
        "trash",
        "deleted",
        "deleted items",
        "deleted messages",
        "papierkorb",
        "gelöschte elemente",
        "gelöscht",
    ]) {
        Some(FolderRole::Trash)
    } else if matches(&["junk", "spam", "junk e-mail", "junk-e-mail", "junk mail"]) {
        Some(FolderRole::Junk)
    } else if matches(&["archive", "archiv"]) {
        Some(FolderRole::Archive)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_modified_utf7_names() {
        let folder = RemoteFolder::new("Entw&APw-rfe", Some("/"), None, true);
        assert_eq!(folder.name, "Entwürfe");
        assert_eq!(folder.path, "Entw&APw-rfe");
        assert_eq!(folder.role, Some(FolderRole::Drafts));
    }

    #[test]
    fn uses_the_last_path_segment_as_name() {
        let folder = RemoteFolder::new("INBOX.Projekte.2026", Some("."), None, true);
        assert_eq!(folder.name, "2026");
        assert_eq!(folder.role, None);
    }

    #[test]
    fn recognises_inbox_and_common_names() {
        assert_eq!(
            RemoteFolder::new("INBOX", Some("/"), None, true).role,
            Some(FolderRole::Inbox)
        );
        assert_eq!(
            RemoteFolder::new("inbox", None, None, true).role,
            Some(FolderRole::Inbox)
        );
        assert_eq!(
            RemoteFolder::new("Gesendete Elemente", Some("/"), None, true).role,
            Some(FolderRole::Sent)
        );
        assert_eq!(
            RemoteFolder::new("INBOX/Spam", Some("/"), None, true).role,
            Some(FolderRole::Junk)
        );
    }

    #[test]
    fn attributes_win_over_names() {
        let folder = RemoteFolder::new("Ablage", Some("/"), Some(FolderRole::Archive), true);
        assert_eq!(folder.role, Some(FolderRole::Archive));
    }
}
