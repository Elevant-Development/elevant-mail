//! Moving accounts to another computer: a file with each account's settings (servers, colour,
//! sender name, signature, how much stays offline) and, only with a passphrase, its passwords
//! and Microsoft sign-in, encrypted (Argon2id, AES-256-GCM). No mail: the other computer fetches
//! it from the servers.

use std::collections::HashMap;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ring::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::accounts::{self, NewAccount, SignInMethod};
use crate::db::Database;
use crate::ids::now_millis;
use crate::mail::settings::{ImapSettings, SmtpSettings};
use crate::model::Account;
use crate::secrets::{Secret, SecretKind, SecretStore};
use crate::{Error, Result};

const FORMAT: &str = "elevant-mail-accounts";
const VERSION: u32 = 1;

/// The file as written.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportFile {
    format: String,
    version: u32,
    exported_at: i64,
    accounts: Vec<ExportedAccount>,
    image_senders: Vec<String>,
    /// The sign-ins, encrypted with the passphrase; none without one.
    sealed: Option<Sealed>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExportedAccount {
    display_name: String,
    email_address: String,
    color: String,
    sign_in: SignInMethod,
    imap: ImapSettings,
    smtp: SmtpSettings,
    sender_name: String,
    signature: String,
    signature_html: bool,
    keep_days: Option<u32>,
    in_unified_inbox: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Sealed {
    salt: String,
    nonce: String,
    data: String,
}

/// One account's sign-in, as sealed in the file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Access {
    imap_password: Option<String>,
    smtp_password: Option<String>,
    refresh_token: Option<String>,
}

/// What the import dialog shows of an account in the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportedAccount {
    pub email_address: String,
    pub display_name: String,
    pub color: String,
    pub sign_in: SignInMethod,
    /// Already set up here: skipped.
    pub exists: bool,
    /// Its sign-in came along (after the passphrase opened the file).
    pub has_access: bool,
}

/// The file's accounts, and whether a passphrase is needed for their sign-ins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportPreview {
    pub accounts: Vec<ImportedAccount>,
    /// Sign-ins came along, encrypted; the passphrase opens them.
    pub sealed: bool,
}

/// A file read for importing, with the sign-ins once the passphrase opened them.
pub struct Import {
    file: ExportFile,
    access: HashMap<String, Access>,
}

/// All accounts as a file; with a passphrase, their sign-ins too, encrypted.
pub fn export(
    conn: &Connection,
    secrets: &dyn SecretStore,
    passphrase: Option<&Secret>,
) -> Result<String> {
    let mut exported = Vec::new();
    let mut access = HashMap::new();
    for account in accounts::list(conn)? {
        let settings = accounts::settings(conn, &account.id)?;
        let stored_sender: Option<String> = conn.query_row(
            "SELECT sender_name FROM accounts WHERE id = ?1",
            [&account.id],
            |row| row.get(0),
        )?;
        if passphrase.is_some() {
            let get = |kind| {
                secrets
                    .get(&account.id, kind)
                    .ok()
                    .map(|s: Secret| s.expose().to_owned())
            };
            access.insert(
                account.email_address.to_lowercase(),
                Access {
                    imap_password: get(SecretKind::ImapPassword),
                    smtp_password: get(SecretKind::SmtpPassword),
                    refresh_token: get(SecretKind::OAuthRefreshToken),
                },
            );
        }
        exported.push(ExportedAccount {
            display_name: account.display_name,
            email_address: account.email_address,
            color: account.color,
            sign_in: settings.sign_in,
            imap: settings.imap,
            smtp: settings.smtp,
            // Only one that was set: otherwise the other computer takes it from Sent too.
            sender_name: stored_sender.unwrap_or_default(),
            signature: settings.signature,
            signature_html: settings.signature_html,
            keep_days: settings.keep_days,
            in_unified_inbox: account.in_unified_inbox,
        });
    }
    let sealed = match passphrase {
        Some(passphrase) => Some(seal(
            &serde_json::to_vec(&access).expect("sign-ins are valid JSON"),
            passphrase,
        )?),
        None => None,
    };
    let file = ExportFile {
        format: FORMAT.into(),
        version: VERSION,
        exported_at: now_millis(),
        accounts: exported,
        image_senders: crate::image_senders::list(conn)?,
        sealed,
    };
    Ok(serde_json::to_string_pretty(&file).expect("the export is valid JSON"))
}

impl Import {
    pub fn read(text: &str) -> Result<Self> {
        let file: ExportFile = serde_json::from_str(text).map_err(|_| Error::NotAnExport)?;
        if file.format != FORMAT || file.version > VERSION {
            return Err(Error::NotAnExport);
        }
        Ok(Self {
            file,
            access: HashMap::new(),
        })
    }

    /// Opens the sign-ins with the passphrase.
    pub fn unlock(&mut self, passphrase: &Secret) -> Result<()> {
        let Some(sealed) = &self.file.sealed else {
            return Ok(());
        };
        let plain = open(sealed, passphrase)?;
        self.access = serde_json::from_slice(&plain).map_err(|_| Error::WrongPassphrase)?;
        Ok(())
    }

    pub fn preview(&self, conn: &Connection) -> Result<ImportPreview> {
        let here: Vec<String> = accounts::list(conn)?
            .into_iter()
            .map(|a| a.email_address.to_lowercase())
            .collect();
        let accounts =
            self.file
                .accounts
                .iter()
                .map(|account| {
                    let key = account.email_address.to_lowercase();
                    ImportedAccount {
                        email_address: account.email_address.clone(),
                        display_name: account.display_name.clone(),
                        color: account.color.clone(),
                        sign_in: account.sign_in,
                        exists: here.contains(&key),
                        has_access: self.access.get(&key).is_some_and(|access| {
                            match account.sign_in {
                                SignInMethod::Password => access.imap_password.is_some(),
                                SignInMethod::Microsoft => access.refresh_token.is_some(),
                            }
                        }),
                    }
                })
                .collect();
        Ok(ImportPreview {
            accounts,
            sealed: self.file.sealed.is_some(),
        })
    }

    fn account(&self, email: &str) -> Result<&ExportedAccount> {
        self.file
            .accounts
            .iter()
            .find(|a| a.email_address.eq_ignore_ascii_case(email))
            .ok_or(Error::AccountNotFound)
    }

    /// The account as it is to be added, and its password from the file or the one typed in.
    pub fn new_account(&self, email: &str) -> Result<NewAccount> {
        let account = self.account(email)?;
        Ok(NewAccount {
            display_name: account.display_name.clone(),
            email_address: account.email_address.clone(),
            imap: account.imap.clone(),
        })
    }

    pub fn sign_in(&self, email: &str) -> Result<SignInMethod> {
        Ok(self.account(email)?.sign_in)
    }

    /// The password that came along, if any.
    pub fn password(&self, email: &str) -> Option<Secret> {
        self.access
            .get(&email.to_lowercase())?
            .imap_password
            .as_deref()
            .map(Secret::new)
    }

    /// The Microsoft sign-in that came along, if any.
    pub fn refresh_token(&self, email: &str) -> Option<Secret> {
        self.access
            .get(&email.to_lowercase())?
            .refresh_token
            .as_deref()
            .map(Secret::new)
    }

    /// After the account was added (and its login checked): everything else from the file.
    pub fn apply(&self, db: &Database, secrets: &dyn SecretStore, account: &Account) -> Result<()> {
        let exported = self.account(&account.email_address)?;
        let conn = db.conn();
        accounts::update(
            &conn,
            &account.id,
            &accounts::AccountChanges {
                display_name: exported.display_name.clone(),
                color: exported.color.clone(),
            },
        )?;
        conn.execute(
            "UPDATE accounts SET smtp_host = ?2, smtp_port = ?3, smtp_security = ?4,
               smtp_username = ?5, sender_name = ?6, keep_days = ?7, in_unified_inbox = ?8
             WHERE id = ?1",
            params![
                account.id,
                exported.smtp.host,
                exported.smtp.port,
                exported.smtp.security.as_db_str(),
                exported.smtp.username,
                (!exported.sender_name.trim().is_empty()).then_some(&exported.sender_name),
                exported.keep_days,
                exported.in_unified_inbox,
            ],
        )?;
        accounts::set_signature(
            &conn,
            &account.id,
            &exported.signature,
            exported.signature_html,
        )?;
        if let Some(password) = self
            .access
            .get(&account.email_address.to_lowercase())
            .and_then(|access| access.smtp_password.as_deref())
        {
            secrets.set(
                &account.id,
                SecretKind::SmtpPassword,
                &Secret::new(password),
            )?;
        }
        for address in &self.file.image_senders {
            crate::image_senders::allow(&conn, address)?;
        }
        Ok(())
    }
}

fn key(passphrase: &Secret, salt: &[u8]) -> Result<LessSafeKey> {
    let mut key = zeroize::Zeroizing::new([0u8; 32]);
    argon2::Argon2::default()
        .hash_password_into(passphrase.expose().as_bytes(), salt, key.as_mut())
        .map_err(|_| Error::WrongPassphrase)?;
    let unbound =
        UnboundKey::new(&AES_256_GCM, key.as_ref()).map_err(|_| Error::WrongPassphrase)?;
    Ok(LessSafeKey::new(unbound))
}

fn seal(plain: &[u8], passphrase: &Secret) -> Result<Sealed> {
    let random = SystemRandom::new();
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; NONCE_LEN];
    let no_random = |_| std::io::Error::other("no system randomness");
    random.fill(&mut salt).map_err(no_random)?;
    random.fill(&mut nonce).map_err(no_random)?;
    let mut data = plain.to_vec();
    key(passphrase, &salt)?
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(FORMAT),
            &mut data,
        )
        .expect("sealing fits AES-GCM's limits");
    Ok(Sealed {
        salt: STANDARD.encode(salt),
        nonce: STANDARD.encode(nonce),
        data: STANDARD.encode(data),
    })
}

fn open(sealed: &Sealed, passphrase: &Secret) -> Result<Vec<u8>> {
    let decode = |text: &str| STANDARD.decode(text).map_err(|_| Error::NotAnExport);
    let salt = decode(&sealed.salt)?;
    let nonce: [u8; NONCE_LEN] = decode(&sealed.nonce)?
        .try_into()
        .map_err(|_| Error::NotAnExport)?;
    let mut data = decode(&sealed.data)?;
    let plain = key(passphrase, &salt)?
        .open_in_place(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(FORMAT),
            &mut data,
        )
        .map_err(|_| Error::WrongPassphrase)?;
    Ok(plain.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::{Login, save};
    use crate::mail::folders::RemoteFolder;
    use crate::mail::imap::ConnectionReport;
    use crate::mail::settings::Security;
    use crate::secrets::MemoryStore;

    fn add(db: &Database, secrets: &MemoryStore, email: &str) -> Account {
        save(
            db,
            secrets,
            &NewAccount {
                display_name: "Privat".into(),
                email_address: email.into(),
                imap: ImapSettings {
                    host: "imap.example.org".into(),
                    port: 993,
                    security: Security::Tls,
                    username: email.into(),
                },
            },
            Login::Password(&Secret::new("geheim")),
            &ConnectionReport {
                folders: vec![RemoteFolder::new("INBOX", Some("/"), None, true)],
            },
        )
        .unwrap()
    }

    #[test]
    fn accounts_move_with_their_settings_and_only_sealed_passwords() {
        let (laptop, laptop_secrets) =
            (Database::open_in_memory().unwrap(), MemoryStore::default());
        let account = add(&laptop, &laptop_secrets, "alex@example.org");
        accounts::set_signature(&laptop.conn(), &account.id, "<p>Alex</p>", false).unwrap();
        accounts::set_sender_name(&laptop.conn(), &account.id, "Alex Berger").unwrap();
        accounts::set_in_unified_inbox(&laptop.conn(), &account.id, false).unwrap();
        crate::image_senders::allow(&laptop.conn(), "shop@example.net").unwrap();

        // Without a passphrase no password is in the file.
        let plain = export(&laptop.conn(), &laptop_secrets, None).unwrap();
        assert!(!plain.contains("geheim"));
        let import = Import::read(&plain).unwrap();
        let preview = import
            .preview(&Database::open_in_memory().unwrap().conn())
            .unwrap();
        assert!(!preview.sealed);
        assert!(!preview.accounts[0].has_access);

        // With one, sealed: unreadable without it, complete with it.
        let passphrase = Secret::new("Korrekt Pferd Batterie");
        let sealed = export(&laptop.conn(), &laptop_secrets, Some(&passphrase)).unwrap();
        assert!(!sealed.contains("geheim"));
        let mut import = Import::read(&sealed).unwrap();
        assert!(matches!(
            import.unlock(&Secret::new("falsch")),
            Err(Error::WrongPassphrase)
        ));
        import.unlock(&passphrase).unwrap();

        let (pc, pc_secrets) = (Database::open_in_memory().unwrap(), MemoryStore::default());
        let preview = import.preview(&pc.conn()).unwrap();
        assert!(preview.sealed);
        assert_eq!(preview.accounts[0].email_address, "alex@example.org");
        assert!(preview.accounts[0].has_access);
        assert!(!preview.accounts[0].exists);
        assert_eq!(
            import.password("Alex@Example.org").unwrap().expose(),
            "geheim"
        );

        // Added on the PC (here without a login), then the rest from the file.
        let added = add(&pc, &pc_secrets, "alex@example.org");
        import.apply(&pc, &pc_secrets, &added).unwrap();
        let settings = accounts::settings(&pc.conn(), &added.id).unwrap();
        assert_eq!(settings.signature, "<p>Alex</p>");
        assert_eq!(settings.sender_name, "Alex Berger");
        assert!(!accounts::list(&pc.conn()).unwrap()[0].in_unified_inbox);
        assert_eq!(
            crate::image_senders::list(&pc.conn()).unwrap(),
            ["shop@example.net"]
        );
        assert!(import.preview(&pc.conn()).unwrap().accounts[0].exists);
    }

    #[test]
    fn other_files_are_refused() {
        assert!(matches!(Import::read("{}"), Err(Error::NotAnExport)));
        assert!(matches!(Import::read("kein json"), Err(Error::NotAnExport)));
    }
}
