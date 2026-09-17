//! Archive flag stored in the entry JSON payload (`archived`).
//!
//! Upstream MDBX has no first-class archive bit (Avalonia's `is_archived` is
//! SQLite-only). GTK writes `archived` / `archived_at` on the payload so
//! archive/unarchive works through `EntryRepo::update`.

use mdbx_core::model::EntryType;
use mdbx_storage::connection::VaultConnection;
use serde_json::{json, Value};

use crate::io::{list_by_type, load_entry, save_json_entry};
use crate::payload::{
    is_archived, json_string, kind_label, take_payload_json, title_from_bytes,
};
use crate::VaultError;

const ARCHIVE_TYPES: &[EntryType] = &[
    EntryType::Login,
    EntryType::Note,
    EntryType::Card,
    EntryType::Totp,
    EntryType::DocumentRef,
    EntryType::Identity,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchivedItem {
    pub entry_id: String,
    pub kind: String,
    pub kind_label: String,
    pub title: String,
    pub archived_at: String,
}

pub(crate) fn list_archived(conn: &VaultConnection) -> Result<Vec<ArchivedItem>, VaultError> {
    let mut items = Vec::new();
    for entry_type in ARCHIVE_TYPES {
        let mut entries = list_by_type(conn, entry_type.clone())?;
        for entry in &mut entries {
            let value = take_payload_json(entry);
            if !is_archived(&value) {
                continue;
            }
            items.push(ArchivedItem {
                entry_id: entry.entry_id.clone(),
                kind: entry.entry_type.as_str().to_string(),
                kind_label: kind_label(&entry.entry_type).to_string(),
                title: title_from_bytes(entry.title_ct.as_deref()),
                archived_at: json_string(&value, &["archived_at", "archivedAt"]),
            });
        }
    }
    items.sort_by(|left, right| {
        right
            .archived_at
            .cmp(&left.archived_at)
            .then_with(|| left.title.cmp(&right.title))
    });
    Ok(items)
}

pub(crate) fn set_archived(
    conn: &VaultConnection,
    entry_id: &str,
    archived: bool,
) -> Result<ArchivedItem, VaultError> {
    let mut entry = load_entry(conn, entry_id, false)?;
    let mut value = take_payload_json(&mut entry);
    if !value.is_object() {
        value = json!({});
    }
    if let Some(map) = value.as_object_mut() {
        map.insert("archived".into(), Value::Bool(archived));
        if archived {
            map.insert(
                "archived_at".into(),
                Value::String(chrono::Utc::now().to_rfc3339()),
            );
        } else {
            map.remove("archived_at");
        }
    }
    let title = title_from_bytes(entry.title_ct.as_deref());
    let saved = save_json_entry(
        conn,
        Some(entry_id),
        entry.entry_type.clone(),
        &title,
        &value,
    )?;
    Ok(ArchivedItem {
        entry_id: saved.entry_id,
        kind: saved.entry_type.as_str().to_string(),
        kind_label: kind_label(&saved.entry_type).to_string(),
        title,
        archived_at: json_string(&value, &["archived_at"]),
    })
}
