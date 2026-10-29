//! Links from mail open in the system browser (or mail program), never in the app.

use elevant_mail_core::Error;
use tauri::Url;

use crate::error::CommandError;
use crate::window::is_mail_link;

#[tauri::command]
pub async fn open_link(url: String) -> Result<(), CommandError> {
    let url = Url::parse(&url).map_err(|_| Error::InvalidLink)?;
    if !is_mail_link(&url) {
        return Err(Error::InvalidLink.into());
    }
    open::that_detached(url.as_str()).map_err(Error::OpenLink)?;
    Ok(())
}
