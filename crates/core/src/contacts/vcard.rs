//! Contacts as vCards (RFC 6350, and 3.0 as most servers write it). `calcard` reads them; changes
//! are made to the text itself: a field the dialog did not change keeps its lines as they were,
//! and properties the app does not edit (photos, groups, `X-` extensions) stay byte for byte.
//! `calcard`'s writer would rewrite all of them, and loses inline photos.

use calcard::vcard::{
    VCard, VCardEntry, VCardParameterName, VCardParameterValue, VCardProperty, VCardType,
};

use super::{ContactInput, ContactLabel, Labeled, PostalAddress};
use crate::calendar::PRODUCT_ID;

/// What the dialog shows and edits of a vCard.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Fields {
    pub full_name: String,
    pub given_name: String,
    pub family_name: String,
    pub company: String,
    pub emails: Vec<Labeled>,
    pub phones: Vec<Labeled>,
    pub addresses: Vec<PostalAddress>,
    /// `YYYY-MM-DD`, or `--MM-DD` without a year.
    pub birthday: Option<String>,
    pub note: String,
}

impl Fields {
    /// The name lists show: the full name, else the parts, else the company, else an address.
    pub fn display_name(&self) -> String {
        let parts = [self.given_name.trim(), self.family_name.trim()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        [self.full_name.trim(), parts.as_str(), self.company.trim()]
            .into_iter()
            .find(|name| !name.is_empty())
            .map(str::to_owned)
            .or_else(|| self.emails.first().map(|email| email.value.clone()))
            .unwrap_or_default()
    }
}

/// The vCard's `UID`.
pub(crate) fn uid_of(text: &str) -> Option<String> {
    VCard::parse(text)
        .ok()?
        .uid()
        .map(str::trim)
        .filter(|uid| !uid.is_empty())
        .map(str::to_owned)
}

/// The fields of a vCard; `None` when the text is none.
pub(crate) fn read(text: &str) -> Option<Fields> {
    let card = VCard::parse(text).ok()?;
    let texts = |entry: &VCardEntry| -> Vec<String> {
        entry
            .values
            .iter()
            .map(|value| value.as_text().unwrap_or_default().trim().to_owned())
            .collect()
    };
    let first = |property: VCardProperty| -> String {
        card.property(&property)
            .and_then(|entry| entry.values.first())
            .and_then(|value| value.as_text())
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    let name = card
        .property(&VCardProperty::N)
        .map(texts)
        .unwrap_or_default();
    let part = |index: usize| name.get(index).cloned().unwrap_or_default();
    let labeled = |property: VCardProperty, phone: bool| -> Vec<Labeled> {
        card.properties(&property)
            .filter_map(|entry| {
                let value = entry.values.first()?.as_text()?.trim();
                // `tel:` URIs (vCard 4) are shown as the number.
                let value = if phone {
                    value.strip_prefix("tel:").unwrap_or(value)
                } else {
                    value
                };
                (!value.is_empty()).then(|| Labeled {
                    label: label_of(entry, phone),
                    value: value.to_owned(),
                })
            })
            .collect()
    };
    let addresses = card
        .properties(&VCardProperty::Adr)
        .map(|entry| {
            let parts = texts(entry);
            let part = |index: usize| parts.get(index).cloned().unwrap_or_default();
            // Post office box and extended address go with the street.
            let street = [part(0), part(1), part(2)]
                .into_iter()
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            PostalAddress {
                label: label_of(entry, false),
                street,
                city: part(3),
                region: part(4),
                postal_code: part(5),
                country: part(6),
            }
        })
        .filter(|address| !address.is_empty())
        .collect();
    let birthday = card
        .property(&VCardProperty::Bday)
        .and_then(|entry| entry.values.first())
        .and_then(|value| value.as_partial_date_time())
        .and_then(|date| {
            let (month, day) = (date.month?, date.day?);
            Some(match date.year {
                Some(year) => format!("{year:04}-{month:02}-{day:02}"),
                None => format!("--{month:02}-{day:02}"),
            })
        });
    Some(Fields {
        full_name: first(VCardProperty::Fn),
        given_name: part(1),
        family_name: part(0),
        company: card
            .property(&VCardProperty::Org)
            .and_then(|entry| entry.values.first())
            .and_then(|value| value.as_text())
            .unwrap_or_default()
            .trim()
            .to_owned(),
        emails: labeled(VCardProperty::Email, false),
        phones: labeled(VCardProperty::Tel, true),
        addresses,
        birthday,
        note: first(VCardProperty::Note),
    })
}

fn label_of(entry: &VCardEntry, phone: bool) -> ContactLabel {
    let types: Vec<&VCardParameterValue> = entry
        .params
        .iter()
        .filter(|param| param.name == VCardParameterName::Type)
        .map(|param| &param.value)
        .collect();
    let has = |wanted: VCardType| {
        types
            .iter()
            .any(|value| matches!(value, VCardParameterValue::Type(kind) if *kind == wanted))
    };
    if phone && has(VCardType::Cell) {
        ContactLabel::Mobile
    } else if has(VCardType::Work) {
        ContactLabel::Work
    } else if has(VCardType::Home) {
        ContactLabel::Home
    } else {
        ContactLabel::Other
    }
}

/// The fields the dialog left, as the vCard will hold them.
pub(crate) fn fields_of(input: &ContactInput) -> Fields {
    let labeled = |values: &[Labeled]| -> Vec<Labeled> {
        values
            .iter()
            .map(|labeled| Labeled {
                label: labeled.label,
                value: labeled.value.trim().to_owned(),
            })
            .filter(|labeled| !labeled.value.is_empty())
            .collect()
    };
    let mut fields = Fields {
        full_name: String::new(),
        given_name: input.given_name.trim().to_owned(),
        family_name: input.family_name.trim().to_owned(),
        company: input.company.trim().to_owned(),
        emails: labeled(&input.emails),
        phones: labeled(&input.phones),
        addresses: input
            .addresses
            .iter()
            .map(PostalAddress::trimmed)
            .filter(|address| !address.is_empty())
            .collect(),
        birthday: input.birthday.clone().filter(|day| !day.trim().is_empty()),
        note: input.note.trim().to_owned(),
    };
    fields.full_name = fields.display_name();
    fields
}

/// Which properties a field is written as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Name,
    Company,
    Emails,
    Phones,
    Addresses,
    Birthday,
    Note,
}

impl Field {
    const ALL: [Field; 7] = [
        Field::Name,
        Field::Company,
        Field::Emails,
        Field::Phones,
        Field::Addresses,
        Field::Birthday,
        Field::Note,
    ];

    fn properties(self) -> &'static [&'static str] {
        match self {
            Field::Name => &["FN", "N"],
            Field::Company => &["ORG"],
            Field::Emails => &["EMAIL"],
            Field::Phones => &["TEL"],
            Field::Addresses => &["ADR"],
            Field::Birthday => &["BDAY"],
            Field::Note => &["NOTE"],
        }
    }

    fn changed(self, old: &Fields, new: &Fields) -> bool {
        match self {
            Field::Name => {
                (&old.given_name, &old.family_name) != (&new.given_name, &new.family_name)
                    // A card with only a company has its name from it.
                    || (old.given_name.is_empty()
                        && old.family_name.is_empty()
                        && old.display_name() != new.display_name())
            }
            Field::Company => old.company != new.company,
            Field::Emails => old.emails != new.emails,
            Field::Phones => old.phones != new.phones,
            Field::Addresses => old.addresses != new.addresses,
            Field::Birthday => old.birthday != new.birthday,
            Field::Note => old.note != new.note,
        }
    }

    fn lines(self, fields: &Fields, version_4: bool) -> Vec<String> {
        let kind = |label: ContactLabel, phone: bool| -> &'static str {
            match (label, phone) {
                (ContactLabel::Mobile, _) => "CELL",
                (ContactLabel::Home, _) => "HOME",
                (ContactLabel::Work, _) => "WORK",
                (ContactLabel::Other, true) => "VOICE",
                (ContactLabel::Other, false) => "",
            }
        };
        match self {
            Field::Name => vec![
                format!("FN:{}", escape(&fields.full_name)),
                format!(
                    "N:{};{};;;",
                    escape(&fields.family_name),
                    escape(&fields.given_name)
                ),
            ],
            Field::Company => (!fields.company.is_empty())
                .then(|| format!("ORG:{}", escape(&fields.company)))
                .into_iter()
                .collect(),
            Field::Emails => fields
                .emails
                .iter()
                .map(|email| {
                    let mut types = Vec::new();
                    if !version_4 {
                        types.push("INTERNET");
                    }
                    let label = kind(email.label, false);
                    if !label.is_empty() {
                        types.push(label);
                    }
                    let types = if types.is_empty() {
                        String::new()
                    } else {
                        format!(";TYPE={}", types.join(","))
                    };
                    format!("EMAIL{types}:{}", escape(&email.value))
                })
                .collect(),
            Field::Phones => fields
                .phones
                .iter()
                .map(|phone| {
                    format!(
                        "TEL;TYPE={}:{}",
                        kind(phone.label, true),
                        escape(&phone.value)
                    )
                })
                .collect(),
            Field::Addresses => fields
                .addresses
                .iter()
                .map(|address| {
                    let label = kind(address.label, false);
                    let types = if label.is_empty() {
                        String::new()
                    } else {
                        format!(";TYPE={label}")
                    };
                    let street = address.street.lines().collect::<Vec<_>>().join(", ");
                    format!(
                        "ADR{types}:;;{};{};{};{};{}",
                        escape(&street),
                        escape(&address.city),
                        escape(&address.region),
                        escape(&address.postal_code),
                        escape(&address.country),
                    )
                })
                .collect(),
            Field::Birthday => fields
                .birthday
                .iter()
                .filter_map(|day| birthday_value(day))
                .map(|value| format!("BDAY:{value}"))
                .collect(),
            Field::Note => (!fields.note.is_empty())
                .then(|| format!("NOTE:{}", escape(&fields.note)))
                .into_iter()
                .collect(),
        }
    }
}

/// `text` when it is `digits` digits.
fn number(text: &str, digits: usize) -> Option<&str> {
    (text.len() == digits && text.bytes().all(|byte| byte.is_ascii_digit())).then_some(text)
}

/// `BDAY` from `YYYY-MM-DD` or `--MM-DD`; `None` for anything else.
fn birthday_value(day: &str) -> Option<String> {
    let day = day.trim();
    let (year, rest) = match day.strip_prefix("--") {
        Some(rest) => (None, rest),
        None => {
            let (year, rest) = day.split_once('-')?;
            (Some(year), rest)
        }
    };
    let (month, day) = rest.split_once('-')?;
    let (month, day) = (number(month, 2)?, number(day, 2)?);
    Some(match year {
        Some(year) => format!("{}-{month}-{day}", number(year, 4)?),
        None => format!("--{month}{day}"),
    })
}

/// A new vCard (3.0, which every server takes) with these fields.
pub(crate) fn new_card(uid: &str, fields: &Fields, revision: &str) -> String {
    let mut lines = vec![
        "BEGIN:VCARD".to_owned(),
        "VERSION:3.0".to_owned(),
        format!("PRODID:{PRODUCT_ID}"),
        format!("UID:{}", escape(uid)),
    ];
    for field in Field::ALL {
        lines.extend(field.lines(fields, false));
    }
    lines.push(format!("REV:{revision}"));
    lines.push("END:VCARD".to_owned());
    join(&lines)
}

/// The vCard with the fields as the dialog left them: lines of unchanged fields and of everything
/// else stay as they are.
pub(crate) fn changed_card(text: &str, old: &Fields, new: &Fields, revision: &str) -> String {
    let lines = unfold(text);
    let version_4 = lines
        .iter()
        .any(|line| line.eq_ignore_ascii_case("VERSION:4.0"));
    let changed: Vec<Field> = Field::ALL
        .into_iter()
        .filter(|field| field.changed(old, new))
        .collect();
    let replaced = |name: &str| {
        name == "REV"
            || changed
                .iter()
                .any(|field| field.properties().contains(&name))
    };
    // Groups (`item1.EMAIL`, with `item1.X-ABLabel`) of replaced lines go with them.
    let dropped_groups: Vec<String> = lines
        .iter()
        .filter_map(|line| {
            let (group, name) = name_of(line);
            group.filter(|_| replaced(&name)).map(str::to_owned)
        })
        .collect();
    let mut kept: Vec<String> = lines
        .into_iter()
        .filter(|line| {
            let (group, name) = name_of(line);
            !replaced(&name)
                && group.is_none_or(|group| {
                    !dropped_groups
                        .iter()
                        .any(|dropped| dropped.eq_ignore_ascii_case(group))
                })
        })
        .collect();
    let end = kept
        .iter()
        .rposition(|line| line.eq_ignore_ascii_case("END:VCARD"))
        .unwrap_or(kept.len());
    let mut fresh: Vec<String> = changed
        .iter()
        .flat_map(|field| field.lines(new, version_4))
        .collect();
    fresh.push(format!("REV:{revision}"));
    kept.splice(end..end, fresh);
    join(&kept)
}

/// The group and the upper-case name of a content line.
fn name_of(line: &str) -> (Option<&str>, String) {
    let head = line.split([';', ':']).next().unwrap_or_default();
    match head.split_once('.') {
        Some((group, name)) => (Some(group), name.to_ascii_uppercase()),
        None => (None, head.to_ascii_uppercase()),
    }
}

/// The content lines of a vCard, folded lines joined again.
fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        match (line.strip_prefix([' ', '\t']), lines.last_mut()) {
            (Some(rest), Some(last)) => last.push_str(rest),
            _ if line.is_empty() => {}
            _ => lines.push(line.to_owned()),
        }
    }
    lines
}

/// Lines with CRLF, folded at 75 octets as RFC 6350 asks, never inside a character.
fn join(lines: &[String]) -> String {
    let mut out = String::new();
    for line in lines {
        let mut rest = line.as_str();
        let mut width = 75;
        while rest.len() > width {
            let mut cut = width;
            while !rest.is_char_boundary(cut) {
                cut -= 1;
            }
            out.push_str(&rest[..cut]);
            out.push_str("\r\n ");
            rest = &rest[cut..];
            // The space that starts a folded line counts.
            width = 74;
        }
        out.push_str(rest);
        out.push_str("\r\n");
    }
    out
}

/// A text value as a vCard writes it.
fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace("\r\n", "\\n")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANNA: &str = "BEGIN:VCARD\r\nVERSION:3.0\r\nUID:abc\r\nFN:Dr. Anna Müller\r\n\
N:Müller;Anna;Maria;Dr.;\r\nORG:Elevant GmbH;Vertrieb\r\n\
item1.EMAIL;TYPE=INTERNET,WORK,pref:anna@elevant.dev\r\nitem1.X-ABLabel:Büro\r\n\
EMAIL;TYPE=HOME:anna@example.org\r\nTEL;TYPE=CELL:+49 170 1234567\r\n\
ADR;TYPE=WORK:;;Hauptstr. 3;Freiburg;BW;79098;Deutschland\r\nBDAY:1985-04-12\r\n\
NOTE:Zeile eins\\nZeile zwei\r\nX-CUSTOM:bleibt\r\nPHOTO;ENCODING=b;TYPE=JPEG:AAAA\r\n\x20BBBB\r\nREV:20200101T000000Z\r\nEND:VCARD\r\n";

    #[test]
    fn fields_are_read() {
        let fields = read(ANNA).unwrap();
        assert_eq!(fields.display_name(), "Dr. Anna Müller");
        assert_eq!(
            (fields.given_name.as_str(), fields.family_name.as_str()),
            ("Anna", "Müller")
        );
        assert_eq!(fields.company, "Elevant GmbH");
        assert_eq!(
            fields.emails,
            [
                Labeled {
                    label: ContactLabel::Work,
                    value: "anna@elevant.dev".into()
                },
                Labeled {
                    label: ContactLabel::Home,
                    value: "anna@example.org".into()
                },
            ]
        );
        assert_eq!(fields.phones[0].label, ContactLabel::Mobile);
        assert_eq!(fields.addresses[0].city, "Freiburg");
        assert_eq!(fields.addresses[0].postal_code, "79098");
        assert_eq!(fields.birthday.as_deref(), Some("1985-04-12"));
        assert_eq!(fields.note, "Zeile eins\nZeile zwei");
    }

    #[test]
    fn a_change_keeps_everything_it_does_not_touch() {
        let old = read(ANNA).unwrap();
        let mut new = old.clone();
        new.phones[0].value = "+49 171 7654321".into();
        let text = changed_card(ANNA, &old, &new, "20261101T120000Z");
        // Untouched: name with its prefix, company with its department, the grouped address
        // with its label, the photo, extensions.
        for kept in [
            "N:Müller;Anna;Maria;Dr.;",
            "ORG:Elevant GmbH;Vertrieb",
            "item1.EMAIL;TYPE=INTERNET,WORK,pref:anna@elevant.dev",
            "item1.X-ABLabel:Büro",
            "X-CUSTOM:bleibt",
            "PHOTO;ENCODING=b;TYPE=JPEG:AAAABBBB",
        ] {
            assert!(text.contains(kept), "{kept} in {text}");
        }
        assert!(text.contains("TEL;TYPE=CELL:+49 171 7654321"), "{text}");
        assert!(!text.contains("1234567"), "{text}");
        assert!(
            text.contains("REV:20261101T120000Z") && !text.contains("REV:2020"),
            "{text}"
        );
        assert!(text.ends_with("END:VCARD\r\n"), "{text}");
        assert_eq!(read(&text).unwrap().phones[0].value, "+49 171 7654321");

        // New addresses drop the old ones with their group.
        let mut emails = new.clone();
        emails.emails.remove(0);
        let text = changed_card(&text, &new, &emails, "20261101T120001Z");
        assert!(!text.contains("item1."), "{text}");
        assert_eq!(read(&text).unwrap().emails, emails.emails);
    }

    #[test]
    fn new_cards_read_back_and_fold_long_lines() {
        let fields = Fields {
            given_name: "Jonas".into(),
            family_name: "Weber".into(),
            company: "Weber; Söhne".into(),
            emails: vec![Labeled {
                label: ContactLabel::Work,
                value: "jonas@example.com".into(),
            }],
            phones: vec![Labeled {
                label: ContactLabel::Other,
                value: "0761 1".into(),
            }],
            addresses: vec![PostalAddress {
                label: ContactLabel::Home,
                street: "Lange Straße 1".into(),
                postal_code: "79100".into(),
                city: "Freiburg".into(),
                region: String::new(),
                country: String::new(),
            }],
            birthday: Some("--07-31".into()),
            note: "ä".repeat(60),
            ..Fields::default()
        };
        let fields = Fields {
            full_name: fields.display_name(),
            ..fields
        };
        let text = new_card("uid-1", &fields, "20261101T120000Z");
        assert!(text.lines().all(|line| line.len() <= 76), "{text}");
        assert!(text.contains("BDAY:--0731"), "{text}");
        let back = read(&text).unwrap();
        assert_eq!(back, fields);
    }

    #[test]
    fn birthdays_take_only_dates() {
        assert_eq!(birthday_value("1985-04-12").as_deref(), Some("1985-04-12"));
        assert_eq!(birthday_value("--04-12").as_deref(), Some("--0412"));
        assert_eq!(birthday_value("12.04.1985"), None);
    }
}
