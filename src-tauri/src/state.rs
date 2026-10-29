//! Shared state of the running app, created once at startup.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use elevant_mail_core::Database;
use elevant_mail_core::mail::remote::RemoteImages;
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
        // Keyring entries are grouped under the app identifier, e.g. "dev.elevant.mail".
        let secrets = Box::new(KeyringStore::new(app.config().identifier.clone()));

        Ok(Self {
            db,
            secrets,
            keyring_available,
            account_locks: Mutex::default(),
            remote_images: RemoteImages::new()?,
            allowed_remote_images: Mutex::default(),
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

    pub fn account_lock(&self, account_id: &str) -> Arc<AsyncMutex<()>> {
        self.account_locks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(account_id.to_owned())
            .or_default()
            .clone()
    }
}
