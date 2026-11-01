//! Address books and their contacts in the cache. The contacts area reads only from here; the
//! sources (CardDAV, Microsoft Graph) fill it. A contact keeps its vCard.
//!
//! Changes take effect here at once. Contacts of an address book on a server are marked
//! `pending` until the sync has sent them; the address book on this computer (`local`) has none.

pub(crate) mod graph;
pub mod sync;
mod vcard;

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{new_id, now_millis};
use crate::{Error, Result};

/// The address book on this computer, made by the migration.
pub const LOCAL_ADDRESS_BOOK: &str = "local";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AddressBook {
    pub id: String,
    /// `None` for the address book on this computer.
    pub account_id: Option<String>,
    /// Empty for the address book on this computer: the interface names it.
    pub name: String,
    pub read_only: bool,
}

/// A contact in a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ContactSummary {
    pub id: String,
    pub address_book_id: String,
    pub name: String,
    pub company: Option<String>,
    /// The first mail address.
    pub email: Option<String>,
}

/// What a mail address, number or postal address is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ContactLabel {
    Home,
    Work,
    Mobile,
    Other,
}

/// A mail address or phone number with what it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Labeled {
    pub label: ContactLabel,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PostalAddress {
    pub label: ContactLabel,
    /// Lines of the street address.
    pub street: String,
    pub postal_code: String,
    pub city: String,
    pub region: String,
    pub country: String,
}

impl PostalAddress {
    fn trimmed(&self) -> Self {
        Self {
            label: self.label,
            street: self
                .street
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join("\n"),
            postal_code: self.postal_code.trim().to_owned(),
            city: self.city.trim().to_owned(),
            region: self.region.trim().to_owned(),
            country: self.country.trim().to_owned(),
        }
    }

    fn is_empty(&self) -> bool {
        [
            &self.street,
            &self.postal_code,
            &self.city,
            &self.region,
            &self.country,
        ]
        .iter()
        .all(|part| part.trim().is_empty())
    }
}

/// A contact as the details and the dialog show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ContactDetails {
    pub id: String,
    pub address_book_id: String,
    pub name: String,
    pub given_name: String,
    pub family_name: String,
    pub company: String,
    pub emails: Vec<Labeled>,
    pub phones: Vec<Labeled>,
    pub addresses: Vec<PostalAddress>,
    /// `YYYY-MM-DD`, or `--MM-DD` without a year.
    pub birthday: Option<String>,
    pub note: String,
    pub read_only: bool,
}

/// A new contact (`id` empty) or the changes to one, from the dialog.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ContactInput {
    pub id: Option<String>,
    pub address_book_id: String,
    pub given_name: String,
    pub family_name: String,
    pub company: String,
    pub emails: Vec<Labeled>,
    pub phones: Vec<Labeled>,
    pub addresses: Vec<PostalAddress>,
    pub birthday: Option<String>,
    pub note: String,
}

pub fn list_address_books(conn: &Connection) -> Result<Vec<AddressBook>> {
    let mut statement = conn.prepare(
        "SELECT b.id, b.account_id, b.name, b.read_only
         FROM address_books b LEFT JOIN accounts a ON a.id = b.account_id
         ORDER BY b.account_id IS NULL, a.sort_order, a.created_at, b.position, b.name",
    )?;
    let books = statement
        .query_map([], |row| {
            Ok(AddressBook {
                id: row.get(0)?,
                account_id: row.get(1)?,
                name: row.get(2)?,
                read_only: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(books)
}

/// The contacts of one address book, or of all with `None`, by name; with a query only those
/// whose name, company, mail addresses or numbers contain each of its words.
pub fn list_contacts(
    conn: &Connection,
    address_book_id: Option<&str>,
    query: &str,
) -> Result<Vec<ContactSummary>> {
    let words: Vec<String> = fold(query)
        .split_whitespace()
        .map(|word| {
            format!(
                "%{}%",
                word.replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            )
        })
        .collect();
    let mut sql = String::from(
        "SELECT id, address_book_id, name, company, email FROM contacts
         WHERE pending IS NOT 'delete' AND (?1 IS NULL OR address_book_id = ?1)",
    );
    for index in 0..words.len() {
        sql.push_str(&format!(" AND search LIKE ?{} ESCAPE '\\'", index + 2));
    }
    sql.push_str(" ORDER BY sort_key, id");
    let mut values: Vec<rusqlite::types::Value> = vec![address_book_id.map(str::to_owned).into()];
    values.extend(words.into_iter().map(Into::into));
    let contacts = conn
        .prepare(&sql)?
        .query_map(rusqlite::params_from_iter(values), |row| {
            Ok(ContactSummary {
                id: row.get(0)?,
                address_book_id: row.get(1)?,
                name: row.get(2)?,
                company: row.get(3)?,
                email: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(contacts)
}

pub fn get_contact(conn: &Connection, contact_id: &str) -> Result<ContactDetails> {
    let (address_book_id, text, read_only): (String, String, bool) = conn
        .query_row(
            "SELECT c.address_book_id, c.vcard, b.read_only FROM contacts c
             JOIN address_books b ON b.id = c.address_book_id
             WHERE c.id = ?1 AND c.pending IS NOT 'delete'",
            [contact_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or(Error::ContactNotFound)?;
    let fields = vcard::read(&text).ok_or(Error::ContactNotFound)?;
    Ok(ContactDetails {
        id: contact_id.to_owned(),
        address_book_id,
        name: fields.display_name(),
        given_name: fields.given_name,
        family_name: fields.family_name,
        company: fields.company,
        emails: fields.emails,
        phones: fields.phones,
        addresses: fields.addresses,
        birthday: fields.birthday,
        note: fields.note,
        read_only,
    })
}

/// Whether contacts can be written to the address book; `true` when it is on a server.
fn writable(conn: &Connection, address_book_id: &str) -> Result<bool> {
    let (account_id, read_only): (Option<String>, bool) = conn
        .query_row(
            "SELECT account_id, read_only FROM address_books WHERE id = ?1",
            [address_book_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(Error::ContactNotFound)?;
    if read_only {
        return Err(Error::AddressBookReadOnly);
    }
    Ok(account_id.is_some())
}

/// Saves a new contact or the changes to one; returns its id. A contact moved to another address
/// book is deleted in the old one and made anew in the other.
pub fn save_contact(conn: &Connection, input: &ContactInput) -> Result<String> {
    let fields = vcard::fields_of(input);
    if fields.full_name.is_empty() {
        return Err(Error::EmptyContact);
    }
    if fields.emails.iter().any(|email| !email.value.contains('@')) {
        return Err(Error::InvalidEmail);
    }
    let remote = writable(conn, &input.address_book_id)?;
    let revision = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let existing: Option<(String, String, String, Option<String>)> = match &input.id {
        Some(id) => Some(
            conn.query_row(
                "SELECT c.uid, c.vcard, c.address_book_id, c.remote_id FROM contacts c
                 JOIN address_books b ON b.id = c.address_book_id
                 WHERE c.id = ?1 AND c.pending IS NOT 'delete' AND b.read_only = 0",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?
            .ok_or(Error::ContactNotFound)?,
        ),
        None => None,
    };
    match existing {
        Some((uid, text, old_book, remote_id)) if old_book == input.address_book_id => {
            let old = vcard::read(&text).ok_or(Error::ContactNotFound)?;
            let text = vcard::changed_card(&text, &old, &fields, &revision);
            let id = input.id.clone().unwrap_or_default();
            let pending = remote.then_some(if remote_id.is_some() {
                "update"
            } else {
                "create"
            });
            store(conn, &id, &uid, &text, &fields, pending, false)?;
            Ok(id)
        }
        Some((_, text, _, remote_id)) => {
            // Moved: gone from the old address book (on its server too), new in the other.
            let id = input.id.clone().unwrap_or_default();
            if remote_id.is_some() {
                conn.execute(
                    "UPDATE contacts SET pending = 'delete', changed_at = ?2 WHERE id = ?1",
                    params![id, now_millis()],
                )?;
            } else {
                conn.execute("DELETE FROM contacts WHERE id = ?1", [&id])?;
            }
            let old = vcard::read(&text).ok_or(Error::ContactNotFound)?;
            let uid = new_id();
            // The other fields go along as they were.
            let text = vcard::changed_card(&text, &old, &fields, &revision)
                .lines()
                .map(|line| {
                    if line.to_ascii_uppercase().starts_with("UID:") {
                        format!("UID:{uid}")
                    } else {
                        line.to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join("\r\n")
                + "\r\n";
            let new = new_id();
            insert(
                conn,
                &new,
                &input.address_book_id,
                &uid,
                &text,
                &fields,
                remote,
            )?;
            Ok(new)
        }
        None => {
            let uid = new_id();
            let text = vcard::new_card(&uid, &fields, &revision);
            let id = new_id();
            insert(
                conn,
                &id,
                &input.address_book_id,
                &uid,
                &text,
                &fields,
                remote,
            )?;
            Ok(id)
        }
    }
}

fn insert(
    conn: &Connection,
    id: &str,
    address_book_id: &str,
    uid: &str,
    text: &str,
    fields: &vcard::Fields,
    remote: bool,
) -> Result<()> {
    conn.execute(
        "INSERT INTO contacts (id, address_book_id, uid, vcard, name, sort_key, search, changed_at)
         VALUES (?1, ?2, ?3, '', '', '', '', 0)",
        params![id, address_book_id, uid],
    )?;
    store(
        conn,
        id,
        uid,
        text,
        fields,
        remote.then_some("create"),
        true,
    )
}

/// Writes a contact's vCard and the columns beside it.
fn store(
    conn: &Connection,
    id: &str,
    uid: &str,
    text: &str,
    fields: &vcard::Fields,
    pending: Option<&str>,
    new: bool,
) -> Result<()> {
    let name = fields.display_name();
    let company = Some(fields.company.clone()).filter(|company| !company.is_empty());
    let mut search = vec![name.clone(), fields.company.clone()];
    search.extend(fields.emails.iter().map(|email| email.value.clone()));
    search.extend(fields.phones.iter().map(|phone| phone.value.clone()));
    conn.execute(
        "UPDATE contacts SET uid = ?2, vcard = ?3, name = ?4, company = ?5, email = ?6,
           sort_key = ?7, search = ?8, changed_at = ?9,
           pending = CASE WHEN ?10 THEN ?11 ELSE coalesce(pending, ?11) END
         WHERE id = ?1",
        params![
            id,
            uid,
            text,
            name,
            company,
            fields.emails.first().map(|email| email.value.clone()),
            fold(&name),
            fold(&search.join(" ")),
            now_millis(),
            new,
            pending,
        ],
    )?;
    conn.execute("DELETE FROM contact_addresses WHERE contact_id = ?1", [id])?;
    for (position, email) in fields.emails.iter().enumerate() {
        conn.execute(
            "INSERT INTO contact_addresses (contact_id, address, position) VALUES (?1, ?2, ?3)",
            params![id, email.value.to_lowercase(), position as i64],
        )?;
    }
    Ok(())
}

/// Takes a contact as the server has it (its address there, version and vCard), replacing the
/// one cached with that address or UID. A vCard that cannot be read is left out.
pub(crate) fn store_remote(
    conn: &Connection,
    address_book_id: &str,
    remote_id: &str,
    etag: Option<&str>,
    text: &str,
) -> Result<()> {
    let Some(fields) = vcard::read(text) else {
        return Ok(());
    };
    let uid = vcard::uid_of(text).unwrap_or_else(|| remote_id.to_owned());
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM contacts WHERE address_book_id = ?1 AND (remote_id = ?2 OR uid = ?3)",
            params![address_book_id, remote_id, uid],
            |row| row.get(0),
        )
        .optional()?;
    let id = match existing {
        Some(id) => id,
        None => {
            let id = new_id();
            insert(conn, &id, address_book_id, &uid, text, &fields, false)?;
            id
        }
    };
    store(conn, &id, &uid, text, &fields, None, true)?;
    conn.execute(
        "UPDATE contacts SET remote_id = ?2, etag = ?3 WHERE id = ?1",
        params![id, remote_id, etag],
    )?;
    Ok(())
}

/// Deletes a contact; one on a server waits for the sync to delete it there.
pub fn delete_contact(conn: &Connection, contact_id: &str) -> Result<()> {
    let (address_book_id, remote_id): (String, Option<String>) = conn
        .query_row(
            "SELECT address_book_id, remote_id FROM contacts WHERE id = ?1",
            [contact_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(Error::ContactNotFound)?;
    writable(conn, &address_book_id)?;
    if remote_id.is_some() {
        conn.execute(
            "UPDATE contacts SET pending = 'delete', changed_at = ?2 WHERE id = ?1",
            params![contact_id, now_millis()],
        )?;
    } else {
        conn.execute("DELETE FROM contacts WHERE id = ?1", [contact_id])?;
    }
    Ok(())
}

/// Lower case without accents, so "Müller" sorts with "Mueller"'s neighbours and "muller" finds it.
pub(crate) fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .flat_map(|character| {
            let plain = match character {
                'ä' | 'à' | 'á' | 'â' | 'ã' | 'å' => "a",
                'ö' | 'ò' | 'ó' | 'ô' | 'õ' | 'ø' => "o",
                'ü' | 'ù' | 'ú' | 'û' => "u",
                'é' | 'è' | 'ê' | 'ë' => "e",
                'í' | 'ì' | 'î' | 'ï' => "i",
                'ç' => "c",
                'ñ' => "n",
                'ß' => "ss",
                _ => "",
            };
            if plain.is_empty() {
                vec![character]
            } else {
                plain.chars().collect()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn input(given: &str, family: &str, email: &str) -> ContactInput {
        ContactInput {
            id: None,
            address_book_id: LOCAL_ADDRESS_BOOK.into(),
            given_name: given.into(),
            family_name: family.into(),
            company: String::new(),
            emails: vec![Labeled {
                label: ContactLabel::Work,
                value: email.into(),
            }],
            phones: Vec::new(),
            addresses: Vec::new(),
            birthday: None,
            note: String::new(),
        }
    }

    #[test]
    fn contacts_are_saved_found_changed_and_deleted() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        assert_eq!(list_address_books(&conn).unwrap()[0].id, LOCAL_ADDRESS_BOOK);
        let anna = save_contact(&conn, &input("Anna", "Müller", "anna@elevant.dev")).unwrap();
        save_contact(&conn, &input("Jonas", "Weber", "jonas@example.com")).unwrap();
        save_contact(&conn, &input("Ümit", "Kaya", "uemit@example.org")).unwrap();

        let names: Vec<String> = list_contacts(&conn, None, "")
            .unwrap()
            .into_iter()
            .map(|contact| contact.name)
            .collect();
        assert_eq!(names, ["Anna Müller", "Jonas Weber", "Ümit Kaya"]);
        // Without accents, by address, every word.
        assert_eq!(list_contacts(&conn, None, "muller").unwrap()[0].id, anna);
        assert_eq!(list_contacts(&conn, None, "ELEVANT").unwrap()[0].id, anna);
        assert_eq!(list_contacts(&conn, None, "anna weber").unwrap().len(), 0);
        assert_eq!(list_contacts(&conn, Some("other"), "").unwrap().len(), 0);
        assert_eq!(list_contacts(&conn, None, "100%").unwrap().len(), 0);

        let mut changed = input("Anna", "Müller-Lang", "anna@elevant.dev");
        changed.id = Some(anna.clone());
        changed.birthday = Some("--04-12".into());
        changed.phones.push(Labeled {
            label: ContactLabel::Mobile,
            value: "+49 170 1".into(),
        });
        assert_eq!(save_contact(&conn, &changed).unwrap(), anna);
        let details = get_contact(&conn, &anna).unwrap();
        assert_eq!(details.name, "Anna Müller-Lang");
        assert_eq!(details.birthday.as_deref(), Some("--04-12"));
        assert_eq!(details.phones.len(), 1);
        assert!(!details.read_only);
        let address: String = conn
            .query_row(
                "SELECT address FROM contact_addresses WHERE contact_id = ?1",
                [&anna],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(address, "anna@elevant.dev");

        delete_contact(&conn, &anna).unwrap();
        assert!(matches!(
            get_contact(&conn, &anna),
            Err(Error::ContactNotFound)
        ));
        assert_eq!(list_contacts(&conn, None, "").unwrap().len(), 2);
    }

    #[test]
    fn empty_contacts_and_wrong_addresses_are_refused() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        let mut empty = input("", "", "");
        empty.emails.clear();
        assert!(matches!(
            save_contact(&conn, &empty),
            Err(Error::EmptyContact)
        ));
        assert!(matches!(
            save_contact(&conn, &input("Anna", "", "anna")),
            Err(Error::InvalidEmail)
        ));
        // A company alone is a name.
        let mut company = empty.clone();
        company.company = "Stadtwerke".into();
        let id = save_contact(&conn, &company).unwrap();
        assert_eq!(get_contact(&conn, &id).unwrap().name, "Stadtwerke");
    }
}
