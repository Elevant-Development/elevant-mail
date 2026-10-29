//! Shared state of the running app, created once at startup.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use elevant_mail_core::Database;
use elevant_mail_core::mail::remote::RemoteImages;
use elevant_mail_core::model::Attachment;
use elevant_mail_core::secrets::{KeyringStore, SecretStore};
use tauri::async_runtime::Mutex as AsyncMutex;
use tauri::{AppHandle, Manager};

pub struct AppState {
    pub db: Database,
    pub secrets: Box<dyn SecretStore>,
    /// Checked once at startup. Without a keyring, accounts cannot store their passwords.
    pub keyring_available: bool,
    /// One mail server operation per account at a time; further requests wait their turn.
    account_locks: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
    pub remote_images: RemoteImages,
    /// External image URLs per message whose images the user allowed in this session.
    allowed_remote_images: Mutex<HashMap<String, Vec<String>>>,
    /// Attachments fetched in this session, as files for opening and saving. Emptied at start.
    attachment_dir: PathBuf,
    attachment_files: Mutex<HashMap<String, PathBuf>>,
}

impl AppState {
    pub fn init(app: &AppHandle) -> Result<Self, Box<dyn std::error::Error>> {
        let db_path = app.path().app_data_dir()?.join("elevant-mail.sqlite3");
        let db = Database::open(&db_path)?;
        tracing::info!(
            path = %db_path.display(),
            schema = db.schema_version()?,
            "database ready"
        );

        let keyring_available = match KeyringStore::status() {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, "system keyring not available, passwords cannot be stored");
                false
            }
        };
        let attachment_dir = app.path().app_cache_dir()?.join("attachments");
        reset_private_dir(&attachment_dir)?;

        // Keyring entries are grouped under the app identifier, e.g. "dev.elevant.mail".
        let secrets = Box::new(KeyringStore::new(app.config().identifier.clone()));

        Ok(Self {
            db,
            secrets,
            keyring_available,
            account_locks: Mutex::default(),
            remote_images: RemoteImages::new()?,
            allowed_remote_images: Mutex::default(),
            attachment_dir,
            attachment_files: Mutex::default(),
        })
    }

    pub fn allow_remote_images(&self, message_id: &str, urls: Vec<String>) {
        self.allowed_remote_images
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(message_id.to_owned(), urls);
    }

    /// The n-th external image of a message, if the user allowed them.
    pub fn allowed_remote_image(&self, message_id: &str, index: usize) -> Option<String> {
        self.allowed_remote_images
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(message_id)?
            .get(index)
            .cloned()
    }

    pub fn cached_attachment(&self, attachment_id: &str) -> Option<PathBuf> {
        self.attachment_files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(attachment_id)
            .filter(|path| path.is_file())
            .cloned()
    }

    /// Writes a fetched attachment into the cache, one folder per attachment so the file keeps
    /// its own name (programs show it, and some go by the extension).
    pub fn cache_attachment(
        &self,
        attachment: &Attachment,
        data: &[u8],
    ) -> Result<PathBuf, elevant_mail_core::Error> {
        let dir = self.attachment_dir.join(&attachment.id);
        std::fs::create_dir_all(&dir).map_err(elevant_mail_core::Error::FileWrite)?;
        let path = dir.join(&attachment.file_name);
        std::fs::write(&path, data).map_err(elevant_mail_core::Error::FileWrite)?;
        self.attachment_files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(attachment.id.clone(), path.clone());
        Ok(path)
    }

    pub fn account_lock(&self, account_id: &str) -> Arc<AsyncMutex<()>> {
        self.account_locks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(account_id.to_owned())
            .or_default()
            .clone()
    }
}

/// Empties the folder (files from the last session) and makes it private to the user.
fn reset_private_dir(dir: &Path) -> std::io::Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
