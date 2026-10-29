//! Shared state of the running app, created once at startup.

use elevant_mail_core::Database;
use elevant_mail_core::secrets::{KeyringStore, SecretStore};
use tauri::{AppHandle, Manager};

pub struct AppState {
    pub db: Database,
    pub secrets: Box<dyn SecretStore>,
    /// Checked once at startup. Without a keyring, accounts cannot store their passwords.
    pub keyring_available: bool,
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
        })
    }
}
