//! Recycle bin: list soft-deleted entries and restore via EntryRepo.

use mdbx_storage::connection::VaultConnection;

use crate::io::{list_deleted, restore_entry as restore_loaded};
use crate::payload::{kind_label, take_payload_json, title_from_bytes};
use crate::VaultError;

/// Permanent purge is gated by TIGA authorization and tombstone retention.
/// `TombstoneRepo::purge` is disabled; `purge_authorized` needs TIGA.
pub const PERMANENT_DELETE_BLOCKED: &str = "永久删除需 TIGA，本地库不可用";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashItem {
    pub entry_id: String,
    pub kind: String,
    pub kind_label: String,
    pub title: String,
    pub deleted_at: String,
}

pub(crate) fn list_trash(conn: &VaultConnection) -> Result<Vec<TrashItem>, VaultError> {
    let mut entries = list_deleted(conn)?;
    let mut items = Vec::with_capacity(entries.len());
    for entry in &mut entries {
        let _ = take_payload_json(entry);
        items.push(TrashItem {
            entry_id: entry.entry_id.clone(),
            kind: entry.entry_type.as_str().to_string(),
            kind_label: kind_label(&entry.entry_type).to_string(),
            title: title_from_bytes(entry.title_ct.as_deref()),
            deleted_at: entry.updated_at.clone(),
        });
    }
    items.sort_by(|left, right| {
        right
            .deleted_at
            .cmp(&left.deleted_at)
            .then_with(|| left.entry_id.cmp(&right.entry_id))
    });
    Ok(items)
}

pub(crate) fn restore_trash_item(
    conn: &VaultConnection,
    entry_id: &str,
) -> Result<TrashItem, VaultError> {
    let restored = restore_loaded(conn, entry_id)?;
    Ok(TrashItem {
        entry_id: restored.entry_id,
        kind: restored.entry_type.as_str().to_string(),
        kind_label: kind_label(&restored.entry_type).to_string(),
        title: title_from_bytes(restored.title_ct.as_deref()),
        deleted_at: restored.updated_at,
    })
}

pub fn permanent_delete_blocked() -> VaultError {
    VaultError::Storage(PERMANENT_DELETE_BLOCKED.to_string())
}
