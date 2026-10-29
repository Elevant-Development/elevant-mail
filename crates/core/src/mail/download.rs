//! Attachment bytes, fetched from the server when the user saves or opens one. The cache keeps
//! only the list; mail is fetched again (read-only, `BODY.PEEK`) and the part taken from it.

use mail_parser::MessageParser;

use crate::attachments::{self, AttachmentSource};
use crate::db::Database;
use crate::mail::imap::{self, COMMAND_TIMEOUT, ImapSession, protocol_error, with_timeout};
use crate::mail::sync::{BODY_TIMEOUT, credentials, fetch_within};
use crate::model::Attachment;
use crate::secrets::SecretStore;
use crate::{Error, Result};

pub struct AttachmentFile {
    pub attachment: Attachment,
    pub account_id: String,
    pub data: Vec<u8>,
}

/// Where an attachment comes from: the account, for the per-account lock, and its listing.
pub fn locate(db: &Database, attachment_id: &str) -> Result<AttachmentSource> {
    attachments::source(&db.conn(), attachment_id)?.ok_or(Error::AttachmentNotFound)
}

pub async fn fetch_attachment(
    db: &Database,
    secrets: &dyn SecretStore,
    attachment_id: &str,
) -> Result<AttachmentFile> {
    let source = locate(db, attachment_id)?;
    let (settings, password) = credentials(db, secrets, &source.account_id)?;
    let connector = imap::platform_connector()?;
    let mut session = imap::open_session(&settings, &password, &connector).await?;
    let result = fetch_in(&mut session, &source).await;
    let _ = with_timeout(COMMAND_TIMEOUT, session.logout()).await;
    Ok(AttachmentFile {
        data: result?,
        account_id: source.account_id,
        attachment: source.attachment,
    })
}

pub(crate) async fn fetch_in(
    session: &mut ImapSession,
    source: &AttachmentSource,
) -> Result<Vec<u8>> {
    with_timeout(COMMAND_TIMEOUT, session.examine(&source.folder_path))
        .await?
        .map_err(protocol_error)?;
    let fetches = fetch_within(session, &[source.uid], "(UID BODY.PEEK[])", BODY_TIMEOUT).await?;
    let raw = fetches
        .iter()
        .find_map(|fetch| fetch.body())
        .ok_or(Error::AttachmentNotFound)?;
    let message = MessageParser::new()
        .parse(raw)
        .ok_or(Error::AttachmentNotFound)?;
    let part = message
        .part(source.part_id)
        .filter(|part| part.is_attachment())
        .ok_or(Error::AttachmentNotFound)?;
    Ok(part.decoded().into_owned())
}

#[cfg(test)]
pub(crate) async fn fetch_with_connector(
    db: &Database,
    secrets: &dyn SecretStore,
    attachment_id: &str,
    connector: &tokio_rustls::TlsConnector,
) -> Result<Vec<u8>> {
    let source = locate(db, attachment_id)?;
    let (settings, password) = credentials(db, secrets, &source.account_id)?;
    let mut session = imap::open_session(&settings, &password, connector).await?;
    let result = fetch_in(&mut session, &source).await;
    let _ = session.logout().await;
    result
}
