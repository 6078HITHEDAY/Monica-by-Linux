//! Login entry payload + CRUD on an unlocked [`VaultConnection`].
//!
//! Payload shape is compatible with upstream Mdbx tests (`username` /
//! `password`) and with Monica Android / Avalonia (`kind=password`,
//! `website`, `password_plain`). List rows never keep the password field.

use mdbx_core::model::{Entry, EntryType};
use mdbx_storage::connection::VaultConnection;
use mdbx_storage::error::StorageError;
use mdbx_storage::repo::{CommitContext, EntryRepo, ProjectRepo};
use secrecy::{ExposeSecret, SecretString};
use zeroize::Zeroize;

use crate::{storage_error, VaultError, DEVICE_ID};

const DEFAULT_PROJECT_TITLE: &str = "Monica";
const UNNAMED_TITLE: &str = "未命名";

/// Non-secret list row. Username and URL come from the decrypted payload;
/// the password field is discarded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordEntrySummary {
    pub entry_id: String,
    pub project_id: String,
    pub title: String,
    pub username: String,
    pub url: String,
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
}

struct LoginFields {
    title: String,
    username: String,
    url: String,
    notes: String,
    password: Option<String>,
}

pub(crate) fn list_password_entries(
    conn: &VaultConnection,
) -> Result<Vec<PasswordEntrySummary>, VaultError> {
    let mut entries = EntryRepo::list_by_type(conn, EntryType::Login).map_err(storage_error)?;
    let mut summaries = Vec::with_capacity(entries.len());
    for entry in &mut entries {
        let fields = LoginFields::from_entry(entry, false)?;
        summaries.push(PasswordEntrySummary {
            entry_id: entry.entry_id.clone(),
            project_id: entry.project_id.clone(),
            title: fields.title,
            username: fields.username,
            url: fields.url,
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
    let mut entry = EntryRepo::get_by_id(conn, entry_id)
        .map_err(storage_error)?
        .ok_or_else(|| VaultError::EntryNotFound(entry_id.to_string()))?;
    if entry.deleted {
        return Err(VaultError::EntryNotFound(entry_id.to_string()));
    }
    if entry.entry_type != EntryType::Login {
        return Err(VaultError::Storage(format!(
            "entry {entry_id} is not a login"
        )));
    }
    let fields = LoginFields::from_entry(&mut entry, true)?;
    Ok(PasswordEntryDetail {
        entry_id: entry.entry_id,
        project_id: entry.project_id,
        title: fields.title,
        username: fields.username,
        url: fields.url,
        notes: fields.notes,
        password: SecretString::from(fields.password.unwrap_or_default()),
        updated_at: entry.updated_at,
    })
}

pub(crate) fn save_password_entry(
    conn: &VaultConnection,
    draft: &PasswordEntryDraft,
) -> Result<PasswordEntrySummary, VaultError> {
    let title = draft.title.trim();
    if title.is_empty() {
        return Err(VaultError::Storage("条目标题不能为空".to_string()));
    }
    let ctx = CommitContext::new(DEVICE_ID.to_string());
    let project_id = ensure_default_project(conn, &ctx)?;
    let payload = encode_login_payload(
        &draft.username,
        &draft.url,
        draft.password.expose_secret(),
        &draft.notes,
    );

    let saved = if let Some(entry_id) = draft.entry_id.as_deref() {
        let mut entry = EntryRepo::get_by_id(conn, entry_id)
            .map_err(storage_error)?
            .ok_or_else(|| VaultError::EntryNotFound(entry_id.to_string()))?;
        if entry.deleted {
            return Err(VaultError::EntryNotFound(entry_id.to_string()));
        }
        entry.title_ct = Some(title.as_bytes().to_vec());
        entry.payload_ct = serde_json::to_vec(&payload)
            .map_err(|error| VaultError::Storage(format!("encode login payload: {error}")))?;
        entry.entry_type = EntryType::Login;
        EntryRepo::update(conn, &ctx, &entry).map_err(storage_error)?
    } else {
        EntryRepo::create(
            conn,
            &ctx,
            &project_id,
            EntryType::Login,
            Some(title),
            &payload,
        )
        .map_err(storage_error)?
    };

    Ok(PasswordEntrySummary {
        entry_id: saved.entry_id,
        project_id: saved.project_id,
        title: title.to_string(),
        username: draft.username.clone(),
        url: draft.url.clone(),
        updated_at: saved.updated_at,
    })
}

pub(crate) fn delete_password_entry(
    conn: &VaultConnection,
    entry_id: &str,
) -> Result<(), VaultError> {
    let ctx = CommitContext::new(DEVICE_ID.to_string());
    EntryRepo::soft_delete(conn, &ctx, entry_id).map_err(|error| match error {
        StorageError::NotFound(_) => VaultError::EntryNotFound(entry_id.to_string()),
        other => storage_error(other),
    })
}

fn ensure_default_project(
    conn: &VaultConnection,
    ctx: &CommitContext,
) -> Result<String, VaultError> {
    let projects = ProjectRepo::list_all(conn).map_err(storage_error)?;
    if let Some(existing) = projects.into_iter().next() {
        return Ok(existing.project_id);
    }
    let created =
        ProjectRepo::create(conn, ctx, DEFAULT_PROJECT_TITLE, None, None).map_err(storage_error)?;
    Ok(created.project_id)
}

fn encode_login_payload(
    username: &str,
    url: &str,
    password: &str,
    notes: &str,
) -> serde_json::Value {
    serde_json::json!({
        "kind": "password",
        "username": username,
        "website": url,
        "url": url,
        "password": password,
        "password_plain": password,
        "notes": notes,
    })
}

impl LoginFields {
    fn from_entry(entry: &mut Entry, include_secret: bool) -> Result<Self, VaultError> {
        let mut payload_bytes = std::mem::take(&mut entry.payload_ct);
        let value = match serde_json::from_slice::<serde_json::Value>(&payload_bytes) {
            Ok(value) => value,
            Err(_) => {
                payload_bytes.zeroize();
                return Ok(Self {
                    title: title_from_bytes(entry.title_ct.as_deref()),
                    username: String::new(),
                    url: String::new(),
                    notes: String::new(),
                    password: include_secret.then(String::new),
                });
            }
        };
        payload_bytes.zeroize();

        let title = title_from_bytes(entry.title_ct.as_deref());
        let username = json_string(&value, &["username", "user"]);
        let url = json_string(&value, &["website", "url", "uri"]);
        let notes = json_string(&value, &["notes", "note"]);
        let password = if include_secret {
            Some(json_string(&value, &["password_plain", "password"]))
        } else {
            None
        };
        drop(value);
        Ok(Self {
            title,
            username,
            url,
            notes,
            password,
        })
    }
}

fn title_from_bytes(bytes: Option<&[u8]>) -> String {
    bytes
        .map(|raw| String::from_utf8_lossy(raw).into_owned())
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| UNNAMED_TITLE.to_string())
}

fn json_string(value: &serde_json::Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(text) = value.get(*key).and_then(|item| item.as_str()) {
            return text.to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_roundtrip_reads_android_and_mdbx_keys() {
        let mut android = Entry {
            entry_id: "e1".into(),
            project_id: "p1".into(),
            entry_type: EntryType::Login,
            title_ct: Some(b"GitHub".to_vec()),
            payload_ct: br#"{"kind":"password","username":"ada","website":"github.com","password_plain":"s3cret","notes":"2fa"}"#.to_vec(),
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
            payload_ct: br#"{"username":"ada","password":"do-not-keep"}"#.to_vec(),
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
        assert!(entry.payload_ct.is_empty());
    }
}
