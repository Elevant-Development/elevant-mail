//! Contacts of Microsoft 365 and Outlook.com over Microsoft Graph: the default contacts folder and
//! the folders in it, their contacts translated to vCard for the cache and back for writing.
//!
//! Graph knows fewer kinds than vCard: mail addresses have none (they come as "other"), and there
//! is one postal address each for home, work and other. A birthday without a year is kept with
//! the year 1604, as Outlook does.

use chrono::Utc;
use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};

use super::vcard::{self, Fields};
use super::{ContactLabel, Labeled, PostalAddress};
use crate::calendar::graph::{Graph, Written};
use crate::{Error, Result};

/// The address book of the default contacts folder, which Graph lists apart from the others.
pub(crate) const DEFAULT_FOLDER: &str = "contacts";
/// The year Outlook gives a birthday without one.
const NO_YEAR: &str = "1604";
/// What the app reads of a contact.
const FIELDS: &str = "id,changeKey,givenName,surname,displayName,companyName,emailAddresses,\
businessPhones,homePhones,mobilePhone,homeAddress,businessAddress,otherAddress,birthday,\
personalNotes";
/// Graph takes at most this many mail addresses, and numbers per kind.
const MAX_EMAILS: usize = 3;
const MAX_PHONES: usize = 2;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GraphFolder {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GraphContact {
    pub id: String,
    pub change_key: Option<String>,
    pub given_name: Option<String>,
    pub surname: Option<String>,
    pub display_name: Option<String>,
    pub company_name: Option<String>,
    #[serde(default)]
    pub email_addresses: Vec<GraphEmail>,
    #[serde(default)]
    pub business_phones: Vec<String>,
    #[serde(default)]
    pub home_phones: Vec<String>,
    pub mobile_phone: Option<String>,
    pub home_address: Option<GraphAddress>,
    pub business_address: Option<GraphAddress>,
    pub other_address: Option<GraphAddress>,
    pub birthday: Option<String>,
    pub personal_notes: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct GraphEmail {
    pub address: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GraphAddress {
    pub street: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub country_or_region: Option<String>,
    pub postal_code: Option<String>,
}

/// Where a folder's contacts are: the default folder, or one inside it.
fn path(folder: &str) -> String {
    if folder == DEFAULT_FOLDER {
        "me/contacts".to_owned()
    } else {
        format!("me/contactFolders/{folder}/contacts")
    }
}

impl Graph {
    /// The contact folders in the default one (which is not among them).
    pub async fn contact_folders(&self) -> Result<Vec<GraphFolder>> {
        let mut url = self.url("me/contactFolders")?;
        url.query_pairs_mut()
            .append_pair("$select", "id,displayName")
            .append_pair("$top", "100");
        self.pages(url).await
    }

    /// All contacts of a folder, with the fields the app reads.
    pub async fn contacts(&self, folder: &str) -> Result<Vec<GraphContact>> {
        let mut url = self.url(&path(folder))?;
        url.query_pairs_mut()
            .append_pair("$select", FIELDS)
            .append_pair("$top", "500");
        self.pages(url).await
    }

    pub async fn create_contact(&self, folder: &str, contact: &Value) -> Result<String> {
        let url = self.url(&path(folder))?;
        let response = self.send(Method::POST, url, None, Some(contact)).await?;
        match response.status().as_u16() {
            200 | 201 => {
                let created: GraphContact = response
                    .json()
                    .await
                    .map_err(|error| Error::Protocol(error.to_string()))?;
                Ok(created.id)
            }
            403 => Err(Error::AddressBookReadOnly),
            code => Err(Error::Protocol(format!(
                "creating a contact answered {code}"
            ))),
        }
    }

    /// Changes a contact, only if it is still at `version` (its `changeKey`).
    pub async fn update_contact(
        &self,
        id: &str,
        version: Option<&str>,
        contact: &Value,
    ) -> Result<Written> {
        let url = self.url(&format!("me/contacts/{id}"))?;
        let response = self
            .send(Method::PATCH, url, version, Some(contact))
            .await?;
        match response.status().as_u16() {
            200 | 204 => Ok(Written::Done(id.to_owned())),
            404 | 409 | 412 => Ok(Written::Conflict),
            403 => Err(Error::AddressBookReadOnly),
            code => Err(Error::Protocol(format!(
                "changing a contact answered {code}"
            ))),
        }
    }

    /// Deletes a contact; gone already counts as done.
    pub async fn delete_contact(&self, id: &str, version: Option<&str>) -> Result<Written> {
        let url = self.url(&format!("me/contacts/{id}"))?;
        let response = self.send(Method::DELETE, url, version, None).await?;
        match response.status().as_u16() {
            200 | 204 | 404 => Ok(Written::Done(id.to_owned())),
            409 | 412 => Ok(Written::Conflict),
            403 => Err(Error::AddressBookReadOnly),
            code => Err(Error::Protocol(format!(
                "deleting a contact answered {code}"
            ))),
        }
    }
}

fn text(value: Option<&String>) -> String {
    value
        .map(|value| value.trim().to_owned())
        .unwrap_or_default()
}

/// A Graph contact as the vCard the cache keeps, with Graph's id as its UID.
pub(crate) fn to_vcard(contact: &GraphContact) -> String {
    let phones = |numbers: &[String], label: ContactLabel| -> Vec<Labeled> {
        numbers
            .iter()
            .map(|number| number.trim())
            .filter(|number| !number.is_empty())
            .map(|number| Labeled {
                label,
                value: number.to_owned(),
            })
            .collect()
    };
    let mut all_phones = phones(contact.mobile_phone.as_slice(), ContactLabel::Mobile);
    all_phones.extend(phones(&contact.business_phones, ContactLabel::Work));
    all_phones.extend(phones(&contact.home_phones, ContactLabel::Home));
    let addresses = [
        (&contact.home_address, ContactLabel::Home),
        (&contact.business_address, ContactLabel::Work),
        (&contact.other_address, ContactLabel::Other),
    ]
    .into_iter()
    .filter_map(|(address, label)| {
        let address = address.as_ref()?;
        let postal = PostalAddress {
            label,
            street: text(address.street.as_ref()),
            postal_code: text(address.postal_code.as_ref()),
            city: text(address.city.as_ref()),
            region: text(address.state.as_ref()),
            country: text(address.country_or_region.as_ref()),
        };
        (!postal.is_empty()).then_some(postal)
    })
    .collect();
    let birthday = contact
        .birthday
        .as_deref()
        .and_then(|day| day.get(..10))
        .map(|day| match day.strip_prefix(NO_YEAR) {
            Some(rest) => format!("-{rest}"),
            None => day.to_owned(),
        });
    let mut fields = Fields {
        full_name: String::new(),
        given_name: text(contact.given_name.as_ref()),
        family_name: text(contact.surname.as_ref()),
        company: text(contact.company_name.as_ref()),
        emails: contact
            .email_addresses
            .iter()
            .filter_map(|email| email.address.as_deref())
            .map(str::trim)
            .filter(|address| !address.is_empty())
            .map(|address| Labeled {
                label: ContactLabel::Other,
                value: address.to_owned(),
            })
            .collect(),
        phones: all_phones,
        addresses,
        birthday,
        note: text(contact.personal_notes.as_ref()),
    };
    fields.full_name = Some(text(contact.display_name.as_ref()))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| fields.display_name());
    let revision = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    vcard::new_card(&contact.id, &fields, &revision)
}

/// What Graph takes to create or change a contact, from its vCard. Every field the app edits goes
/// along, so emptied ones are emptied there too.
pub(crate) fn to_graph(text: &str) -> Option<Value> {
    let fields = vcard::read(text)?;
    let numbers = |label: ContactLabel| -> Vec<&str> {
        fields
            .phones
            .iter()
            .filter(|phone| phone.label == label)
            .map(|phone| phone.value.as_str())
            .collect()
    };
    let mobiles = numbers(ContactLabel::Mobile);
    // One mobile number in Graph; the others and those of no kind count as work.
    let mut business: Vec<&str> = numbers(ContactLabel::Work);
    business.extend(mobiles.iter().skip(1));
    business.extend(numbers(ContactLabel::Other));
    business.truncate(MAX_PHONES);
    let mut home = numbers(ContactLabel::Home);
    home.truncate(MAX_PHONES);
    let address = |label: ContactLabel| -> Value {
        match fields
            .addresses
            .iter()
            .find(|address| address.label == label)
        {
            Some(address) => json!({
                "street": address.street,
                "postalCode": address.postal_code,
                "city": address.city,
                "state": address.region,
                "countryOrRegion": address.country,
            }),
            None => json!({}),
        }
    };
    let birthday = fields
        .birthday
        .as_deref()
        .map(|day| match day.strip_prefix("--") {
            Some(rest) => format!("{NO_YEAR}-{rest}T00:00:00Z"),
            None => format!("{day}T00:00:00Z"),
        });
    Some(json!({
        "givenName": fields.given_name,
        "surname": fields.family_name,
        "displayName": fields.display_name(),
        "companyName": fields.company,
        "emailAddresses": fields
            .emails
            .iter()
            .take(MAX_EMAILS)
            .map(|email| json!({ "address": email.value, "name": fields.display_name() }))
            .collect::<Vec<_>>(),
        "mobilePhone": mobiles.first().copied().unwrap_or_default(),
        "businessPhones": business,
        "homePhones": home,
        "homeAddress": address(ContactLabel::Home),
        "businessAddress": address(ContactLabel::Work),
        "otherAddress": address(ContactLabel::Other),
        "birthday": birthday,
        "personalNotes": fields.note,
    }))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn mira_json() -> Value {
        json!({
            "id": "AAMk-mira",
            "changeKey": "ck-mira",
            "givenName": "Mira",
            "surname": "Schulz",
            "displayName": "Mira Schulz",
            "companyName": "Pretorix",
            "emailAddresses": [{ "name": "Mira Schulz", "address": "mira@pretorix.example" }],
            "businessPhones": ["+49 761 400 22"],
            "homePhones": [],
            "mobilePhone": "+49 170 7",
            "businessAddress": { "street": "Hauptstr. 3", "city": "Freiburg", "postalCode": "79098",
                                 "state": "", "countryOrRegion": "Deutschland" },
            "homeAddress": {},
            "birthday": "1604-07-31T00:00:00Z",
            "personalNotes": "Mag Tee."
        })
    }

    #[test]
    fn contacts_go_to_vcard_and_back() {
        let mira: GraphContact = serde_json::from_value(mira_json()).unwrap();
        let card = to_vcard(&mira);
        assert!(card.contains("UID:AAMk-mira"), "{card}");
        let fields = vcard::read(&card).unwrap();
        assert_eq!(fields.display_name(), "Mira Schulz");
        assert_eq!(fields.company, "Pretorix");
        assert_eq!(fields.emails[0].value, "mira@pretorix.example");
        assert_eq!(
            fields
                .phones
                .iter()
                .map(|phone| (phone.label, phone.value.as_str()))
                .collect::<Vec<_>>(),
            [
                (ContactLabel::Mobile, "+49 170 7"),
                (ContactLabel::Work, "+49 761 400 22")
            ]
        );
        assert_eq!(fields.addresses.len(), 1);
        assert_eq!(fields.addresses[0].label, ContactLabel::Work);
        assert_eq!(fields.birthday.as_deref(), Some("--07-31"));
        assert_eq!(fields.note, "Mag Tee.");

        let body = to_graph(&card).unwrap();
        assert_eq!(body["mobilePhone"], "+49 170 7");
        assert_eq!(body["businessPhones"], json!(["+49 761 400 22"]));
        assert_eq!(body["businessAddress"]["city"], "Freiburg");
        assert_eq!(body["homeAddress"], json!({}));
        assert_eq!(body["birthday"], "1604-07-31T00:00:00Z");
        assert_eq!(
            body["emailAddresses"][0]["address"],
            "mira@pretorix.example"
        );
    }
}
