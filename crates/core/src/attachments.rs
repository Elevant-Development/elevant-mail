//! Attachments of cached mail: what the reader lists, safe file names, and which files the app
//! never opens directly. The bytes are fetched from the server on demand (`mail::download`).

use rusqlite::{Connection, OptionalExtension, params};

use crate::Result;
use crate::ids::new_id;
use crate::mail::parse::{AttachmentPart, InlineImage};
use crate::model::Attachment;

/// File types that run code when opened. They can be saved, but the app never hands them to
/// the system to open (see docs/project-brief.md, "Security").
const EXECUTABLE_EXTENSIONS: &[&str] = &[
    "exe",
    "com",
    "scr",
    "pif",
    "msi",
    "msp",
    "msc",
    "cpl",
    "bat",
    "cmd",
    "ps1",
    "psm1",
    "vbs",
    "vbe",
    "js",
    "jse",
    "wsf",
    "wsh",
    "hta",
    "lnk",
    "reg",
    "inf",
    "scf",
    "url",
    "jar",
    "appx",
    "msix",
    "appref-ms",
    "application",
    "gadget",
    "sh",
    "bash",
    "zsh",
    "csh",
    "ksh",
    "command",
    "run",
    "bin",
    "desktop",
    "appimage",
    "deb",
    "rpm",
    "snap",
    "flatpakref",
    "py",
    "pyw",
    "pl",
    "rb",
    "php",
    "app",
    "pkg",
    "dmg",
    "workflow",
    "apk",
    "iso",
    "img",
    "vhd",
    "vhdx",
    "docm",
    "xlsm",
    "pptm",
    "dotm",
    "xltm",
];

const EXECUTABLE_TYPES: &[&str] = &[
    "application/x-msdownload",
    "application/x-msdos-program",
    "application/x-ms-installer",
    "application/x-executable",
    "application/x-sharedlib",
    "application/x-sh",
    "application/x-shellscript",
    "application/x-desktop",
    "application/x-appimage",
    "application/java-archive",
    "application/vnd.microsoft.portable-executable",
    "text/javascript",
    "application/javascript",
    "application/hta",
];

/// Whether opening the file could run code. Checked on the name and the declared type, since
/// either can lie; macro-enabled Office files count too.
pub fn is_executable(file_name: &str, mime_type: &str) -> bool {
    let extension = file_name
        .rsplit_once('.')
        .map(|(_, extension)| extension.trim().to_ascii_lowercase())
        .unwrap_or_default();
    EXECUTABLE_EXTENSIONS.contains(&extension.as_str())
        || EXECUTABLE_TYPES.contains(&mime_type.to_ascii_lowercase().as_str())
}

/// A name that is safe as a file name on Linux and Windows: no folders, no control or reserved
/// characters, no reserved device names. Falls back to a name from the type.
pub fn safe_file_name(name: Option<&str>, mime_type: &str) -> String {
    let cleaned: String = name
        .unwrap_or_default()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    // Leading dots would hide the file, trailing dots and spaces Windows drops.
    let cleaned = cleaned
        .trim()
        .trim_start_matches('.')
        .trim_end_matches(['.', ' ']);
    let cleaned: String = cleaned.chars().take(180).collect();

    let stem = cleaned
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    if cleaned.is_empty() || reserved {
        return format!("Anhang.{}", extension_for(mime_type));
    }
    cleaned
}

fn extension_for(mime_type: &str) -> &'static str {
    match mime_type.to_ascii_lowercase().as_str() {
        "application/pdf" => "pdf",
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "text/plain" => "txt",
        "text/calendar" => "ics",
        "text/vcard" | "text/x-vcard" => "vcf",
        "message/rfc822" => "eml",
        "application/zip" => "zip",
        _ => "bin",
    }
}

/// Replaces what the cache knows about a message's attachments and inline images.
pub(crate) fn store(
    conn: &Connection,
    message_id: &str,
    inline_images: &[InlineImage],
    attachments: &[AttachmentPart],
) -> Result<()> {
    conn.execute(
        "DELETE FROM attachments WHERE message_id = ?1",
        [message_id],
    )?;
    let mut insert = conn.prepare_cached(
        "INSERT INTO attachments (id, message_id, part_id, filename, mime_type, size_bytes,
           content_id, is_inline, data)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    for image in inline_images {
        insert.execute(params![
            new_id(),
            message_id,
            image.part_id,
            None::<String>,
            image.mime_type,
            i64::try_from(image.data.len()).unwrap_or(i64::MAX),
            image.content_id,
            true,
            image.data
        ])?;
    }
    for attachment in attachments {
        insert.execute(params![
            new_id(),
            message_id,
            attachment.part_id,
            attachment.filename,
            attachment.mime_type,
            i64::try_from(attachment.size).unwrap_or(i64::MAX),
            None::<String>,
            false,
            None::<Vec<u8>>
        ])?;
    }
    Ok(())
}

/// The attachments the reader lists, in the order of the message.
pub fn list(conn: &Connection, message_id: &str) -> Result<Vec<Attachment>> {
    let mut statement = conn.prepare(
        "SELECT id, filename, mime_type, size_bytes FROM attachments
         WHERE message_id = ?1 AND is_inline = 0 ORDER BY CAST(part_id AS INTEGER)",
    )?;
    let attachments = statement
        .query_map([message_id], |row| {
            let filename: Option<String> = row.get(1)?;
            let mime_type: Option<String> = row.get(2)?;
            Ok(attachment(
                row.get(0)?,
                filename.as_deref(),
                mime_type.as_deref().unwrap_or("application/octet-stream"),
                row.get(3)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(attachments)
}

fn attachment(
    id: String,
    filename: Option<&str>,
    mime_type: &str,
    size: Option<i64>,
) -> Attachment {
    let file_name = safe_file_name(filename, mime_type);
    Attachment {
        can_open: !is_executable(&file_name, mime_type),
        file_name,
        id,
        mime_type: mime_type.to_owned(),
        size: size.and_then(|size| u64::try_from(size).ok()).unwrap_or(0),
    }
}

/// Where an attachment lives on the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentSource {
    pub attachment: Attachment,
    pub message_id: String,
    pub account_id: String,
    pub folder_path: String,
    pub uid: u32,
    pub part_id: u32,
}

pub fn source(conn: &Connection, attachment_id: &str) -> Result<Option<AttachmentSource>> {
    Ok(conn
        .query_row(
            "SELECT a.id, a.filename, a.mime_type, a.size_bytes, a.part_id, m.id, m.account_id,
               f.path, m.uid
             FROM attachments a
             JOIN messages m ON m.id = a.message_id
             JOIN folders f ON f.id = m.folder_id
             WHERE a.id = ?1 AND a.is_inline = 0",
            [attachment_id],
            |row| {
                let filename: Option<String> = row.get(1)?;
                let mime_type: Option<String> = row.get(2)?;
                let part_id: String = row.get(4)?;
                Ok(AttachmentSource {
                    attachment: attachment(
                        row.get(0)?,
                        filename.as_deref(),
                        mime_type.as_deref().unwrap_or("application/octet-stream"),
                        row.get(3)?,
                    ),
                    part_id: part_id.parse().unwrap_or(u32::MAX),
                    message_id: row.get(5)?,
                    account_id: row.get(6)?,
                    folder_path: row.get(7)?,
                    uid: row.get(8)?,
                })
            },
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn executables_are_recognised_by_name_or_type() {
        for name in [
            "setup.exe",
            "Rechnung.pdf.exe",
            "install.SH",
            "start.desktop",
            "Tool.AppImage",
            "skript.ps1",
            "Bericht.docm",
            "link.lnk",
        ] {
            assert!(is_executable(name, "application/octet-stream"), "{name}");
        }
        assert!(is_executable("rechnung.pdf", "application/x-msdownload"));
        for name in [
            "Rechnung.pdf",
            "Foto.JPG",
            "Notizen.txt",
            "Archiv.zip",
            "ohne-endung",
        ] {
            assert!(!is_executable(name, "application/pdf"), "{name}");
        }
    }

    #[test]
    fn file_names_are_made_safe() {
        assert_eq!(
            safe_file_name(Some("Rechnung März.pdf"), "application/pdf"),
            "Rechnung März.pdf"
        );
        assert_eq!(
            safe_file_name(Some("../../.bashrc"), "text/plain"),
            "_.._.bashrc"
        );
        assert_eq!(
            safe_file_name(Some("C:\\Windows\\evil.dll"), "x/y"),
            "C__Windows_evil.dll"
        );
        assert_eq!(safe_file_name(Some(".hidden"), "text/plain"), "hidden");
        assert_eq!(
            safe_file_name(Some("a\u{0}b\nc.txt"), "text/plain"),
            "a_b_c.txt"
        );
        assert_eq!(safe_file_name(Some("name. . "), "text/plain"), "name");
        assert_eq!(safe_file_name(Some("CON.txt"), "text/plain"), "Anhang.txt");
        assert_eq!(safe_file_name(Some("com1"), "image/png"), "Anhang.png");
        assert_eq!(safe_file_name(None, "application/pdf"), "Anhang.pdf");
        assert_eq!(safe_file_name(Some("   "), "message/rfc822"), "Anhang.eml");
        assert_eq!(
            safe_file_name(Some(&"x".repeat(400)), "a/b")
                .chars()
                .count(),
            180
        );
    }

    #[test]
    fn lists_attachments_without_inline_images() {
        let db = Database::open_in_memory().unwrap();
        crate::actions::tests::seed(&db);
        let conn = db.conn();
        store(
            &conn,
            "m1",
            &[InlineImage {
                part_id: "3".into(),
                content_id: "logo@x".into(),
                mime_type: "image/png".into(),
                data: vec![1, 2, 3],
            }],
            &[
                AttachmentPart {
                    part_id: "5".into(),
                    filename: Some("setup.exe".into()),
                    mime_type: "application/octet-stream".into(),
                    size: 2048,
                },
                AttachmentPart {
                    part_id: "4".into(),
                    filename: Some("Rechnung.pdf".into()),
                    mime_type: "application/pdf".into(),
                    size: 1024,
                },
            ],
        )
        .unwrap();

        let listed = list(&conn, "m1").unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].file_name, "Rechnung.pdf");
        assert!(listed[0].can_open);
        assert_eq!(listed[1].file_name, "setup.exe");
        assert!(!listed[1].can_open);

        let source = source(&conn, &listed[1].id).unwrap().unwrap();
        assert_eq!(
            (source.part_id, source.uid, source.folder_path.as_str()),
            (5, 1, "INBOX")
        );
        // The inline image is still served for `cid:`.
        assert!(
            crate::mailbox::inline_image(&conn, "m1", "logo@x")
                .unwrap()
                .is_some()
        );
    }
}
