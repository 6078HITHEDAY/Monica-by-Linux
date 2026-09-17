//! Login entry payload + CRUD on an unlocked [`VaultConnection`].
//!
//! Payload shape is compatible with upstream Mdbx tests (`username` /
//! `password`) and with Monica Android / Avalonia (`kind=password`,
//! `website`, `password_plain`). List rows never keep the password field.

use mdbx_core::model::{Entry, EntryType};
use mdbx_storage::connection::VaultConnection;
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;

use crate::io::{list_by_type, load_entry, save_json_entry, soft_delete_entry};
use crate::payload::{is_archived, json_string, take_payload_json, title_from_bytes};
use crate::VaultError;

/// Non-secret list row. Username and URL come from the decrypted payload;
/// the password field is discarded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordEntrySummary {
    pub entry_id: String,
    pub project_id: String,
    pub title: String,
    pub username: String,
    pub url: String,
    pub has_totp: bool,
    pub archived: bool,
    pub updated_at: String,
}

/// Detail record. The password is a [`SecretString`] and should be revealed
/// in the UI only on demand, then dropped.
#[derive(Debug, Clone)]
pub struct PasswordEntryDetail {
    pub entry_id: String,
    pub project_id: String,
    pub title: String,
    pub username: String,
    pub url: String,
    pub notes: String,
    pub password: SecretString,
    pub totp_secret: SecretString,
    pub archived: bool,
    pub updated_at: String,
}

/// Create / update draft. `entry_id = None` creates a new login.
#[derive(Debug, Clone)]
pub struct PasswordEntryDraft {
    pub entry_id: Option<String>,
    pub title: String,
    pub username: String,
    pub url: String,
    pub notes: String,
    pub password: SecretString,
    pub totp_secret: SecretString,
    pub archived: bool,
}

struct LoginFields {
    title: String,
    username: String,
    url: String,
    notes: String,
    password: Option<String>,
    totp_secret: Option<String>,
    archived: bool,
}

pub(crate) fn list_password_entries(
    conn: &VaultConnection,
) -> Result<Vec<PasswordEntrySummary>, VaultError> {
    let mut entries = list_by_type(conn, EntryType::Login)?;
    let mut summaries = Vec::with_capacity(entries.len());
    for entry in &mut entries {
        let fields = LoginFields::from_entry(entry, false)?;
        if fields.archived {
            continue;
        }
        summaries.push(PasswordEntrySummary {
            entry_id: entry.entry_id.clone(),
            project_id: entry.project_id.clone(),
            title: fields.title,
            username: fields.username,
            url: fields.url,
            has_totp: fields.totp_secret.as_deref().is_some_and(|secret| !secret.is_empty()),
            archived: false,
            updated_at: entry.updated_at.clone(),
        });
    }
    summaries.sort_by(|left, right| {
        left.title
            .to_lowercase()
            .cmp(&right.title.to_lowercase())
            .then_with(|| left.entry_id.cmp(&right.entry_id))
    });
    Ok(summaries)
}

pub(crate) fn get_password_entry(
    conn: &VaultConnection,
    entry_id: &str,
) -> Result<PasswordEntryDetail, VaultError> {
    let mut entry = load_entry(conn, entry_id, false)?;
    if entry.entry_type != EntryType::Login {
        return Err(VaultError::Storage(format!(
            "entry {entry_id} is not a login"
        )));
    }
    let fields = LoginFields::from_entry(&mut entry, true)?;
    if fields.archived {
        return Err(VaultError::EntryNotFound(entry_id.to_string()));
    }
    Ok(PasswordEntryDetail {
        entry_id: entry.entry_id,
        project_id: entry.project_id,
        title: fields.title,
        username: fields.username,
        url: fields.url,
        notes: fields.notes,
        password: SecretString::from(fields.password.unwrap_or_default()),
        totp_secret: SecretString::from(fields.totp_secret.unwrap_or_default()),
        archived: false,
        updated_at: entry.updated_at,
    })
}

pub(crate) fn save_password_entry(
    conn: &VaultConnection,
    draft: &PasswordEntryDraft,
) -> Result<PasswordEntrySummary, VaultError> {
    let payload = encode_login_payload(
        &draft.username,
        &draft.url,
        draft.password.expose_secret(),
        &draft.notes,
        draft.totp_secret.expose_secret(),
        draft.archived,
    );
    let saved = save_json_entry(
        conn,
        draft.entry_id.as_deref(),
        EntryType::Login,
        &draft.title,
        &payload,
    )?;
    Ok(PasswordEntrySummary {
        entry_id: saved.entry_id,
        project_id: saved.project_id,
        title: draft.title.trim().to_string(),
        username: draft.username.clone(),
        url: draft.url.clone(),
        has_totp: !draft.totp_secret.expose_secret().trim().is_empty(),
        archived: draft.archived,
        updated_at: saved.updated_at,
    })
}

pub(crate) fn delete_password_entry(
    conn: &VaultConnection,
    entry_id: &str,
) -> Result<(), VaultError> {
    soft_delete_entry(conn, entry_id)
}

fn encode_login_payload(
    username: &str,
    url: &str,
    password: &str,
    notes: &str,
    totp_secret: &str,
    archived: bool,
) -> serde_json::Value {
    let mut payload = json!({
        "kind": "password",
        "username": username,
        "website": url,
        "url": url,
        "password": password,
        "password_plain": password,
        "notes": notes,
        "authenticator_key": totp_secret.trim(),
        "archived": archived,
    });
    if archived {
        payload.as_object_mut().expect("object").insert(
            "archived_at".into(),
            json!(chrono::Utc::now().to_rfc3339()),
        );
    }
    payload
}

impl LoginFields {
    fn from_entry(entry: &mut Entry, include_secret: bool) -> Result<Self, VaultError> {
        let value = take_payload_json(entry);
        let title = title_from_bytes(entry.title_ct.as_deref());
        let username = json_string(&value, &["username", "user"]);
        let url = json_string(&value, &["website", "url", "uri"]);
        let notes = json_string(&value, &["notes", "note"]);
        let archived = is_archived(&value);
        let totp_raw = json_string(
            &value,
            &["authenticator_key", "authenticatorKey", "otp"],
        );
        let password = if include_secret {
            Some(json_string(&value, &["password_plain", "password"]))
        } else {
            None
        };
        let totp_secret = if include_secret {
            Some(totp_raw)
        } else if totp_raw.is_empty() {
            None
        } else {
            Some("1".to_string())
        };
        drop(value);
        Ok(Self {
            title,
            username,
            url,
            notes,
            password,
            totp_secret,
            archived,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload::UNNAMED_TITLE;

    #[test]
    fn payload_roundtrip_reads_android_and_mdbx_keys() {
        let mut android = Entry {
            entry_id: "e1".into(),
            project_id: "p1".into(),
            entry_type: EntryType::Login,
            title_ct: Some(b"GitHub".to_vec()),
            payload_ct: br#"{"kind":"password","username":"ada","website":"github.com","password_plain":"s3cret","notes":"2fa","authenticator_key":"JBSWY3DPEHPK3PXP"}"#.to_vec(),
            payload_schema_version: 1,
            tiga_mode_override: None,
            object_clock: "{}".into(),
            head_commit_id: "c1".into(),
            deleted: false,
            created_at: String::new(),
            updated_at: String::new(),
            created_by_device_id: String::new(),
            updated_by_device_id: String::new(),
        };
        let fields = LoginFields::from_entry(&mut android, true).expect("android payload");
        assert_eq!(fields.title, "GitHub");
        assert_eq!(fields.username, "ada");
        assert_eq!(fields.url, "github.com");
        assert_eq!(fields.notes, "2fa");
        assert_eq!(fields.password.as_deref(), Some("s3cret"));
        assert_eq!(fields.totp_secret.as_deref(), Some("JBSWY3DPEHPK3PXP"));
        assert!(!fields.archived);

        let mut mdbx = Entry {
            payload_ct: br#"{"username":"bob","password":"hunter2"}"#.to_vec(),
            title_ct: None,
            ..android
        };
        let fields = LoginFields::from_entry(&mut mdbx, true).expect("mdbx payload");
        assert_eq!(fields.title, UNNAMED_TITLE);
        assert_eq!(fields.username, "bob");
        assert_eq!(fields.password.as_deref(), Some("hunter2"));
    }

    #[test]
    fn list_parse_does_not_keep_password() {
        let mut entry = Entry {
            entry_id: "e1".into(),
            project_id: "p1".into(),
            entry_type: EntryType::Login,
            title_ct: Some(b"Mail".to_vec()),
            payload_ct: br#"{"username":"ada","password":"do-not-keep","archived":true}"#.to_vec(),
            payload_schema_version: 1,
            tiga_mode_override: None,
            object_clock: "{}".into(),
            head_commit_id: "c1".into(),
            deleted: false,
            created_at: String::new(),
            updated_at: String::new(),
            created_by_device_id: String::new(),
            updated_by_device_id: String::new(),
        };
        let fields = LoginFields::from_entry(&mut entry, false).expect("list parse");
        assert!(fields.password.is_none());
        assert!(fields.archived);
        assert!(entry.payload_ct.is_empty());
    }
}
