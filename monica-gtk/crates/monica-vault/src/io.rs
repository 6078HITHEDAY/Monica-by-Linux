//! Generic EntryRepo create/update used by typed modules.

use mdbx_core::model::{Entry, EntryType};
use mdbx_storage::connection::VaultConnection;
use mdbx_storage::error::StorageError;
use mdbx_storage::repo::{CommitContext, EntryRepo};

use crate::payload::{encode_payload_bytes, require_title};
use crate::project::ensure_default_project;
use crate::{storage_error, VaultError, DEVICE_ID};

pub(crate) fn load_entry(
    conn: &VaultConnection,
    entry_id: &str,
    allow_deleted: bool,
) -> Result<Entry, VaultError> {
    let entry = EntryRepo::get_by_id(conn, entry_id)
        .map_err(storage_error)?
        .ok_or_else(|| VaultError::EntryNotFound(entry_id.to_string()))?;
    if entry.deleted && !allow_deleted {
        return Err(VaultError::EntryNotFound(entry_id.to_string()));
    }
    Ok(entry)
}

pub(crate) fn save_json_entry(
    conn: &VaultConnection,
    entry_id: Option<&str>,
    entry_type: EntryType,
    title: &str,
    payload: &serde_json::Value,
) -> Result<Entry, VaultError> {
    let title = require_title(title)?;
    let ctx = CommitContext::new(DEVICE_ID.to_string());
    let project_id = ensure_default_project(conn, &ctx)?;
    let payload_bytes = encode_payload_bytes(payload)?;

    if let Some(entry_id) = entry_id {
        let mut entry = load_entry(conn, entry_id, false)?;
        entry.title_ct = Some(title.as_bytes().to_vec());
        entry.payload_ct = payload_bytes;
        entry.entry_type = entry_type;
        EntryRepo::update(conn, &ctx, &entry).map_err(storage_error)
    } else {
        EntryRepo::create(
            conn,
            &ctx,
            &project_id,
            entry_type,
            Some(title),
            payload,
        )
        .map_err(storage_error)
    }
}

pub(crate) fn soft_delete_entry(conn: &VaultConnection, entry_id: &str) -> Result<(), VaultError> {
    let ctx = CommitContext::new(DEVICE_ID.to_string());
    EntryRepo::soft_delete(conn, &ctx, entry_id).map_err(|error| match error {
        StorageError::NotFound(_) => VaultError::EntryNotFound(entry_id.to_string()),
        other => storage_error(other),
    })
}

pub(crate) fn restore_entry(conn: &VaultConnection, entry_id: &str) -> Result<Entry, VaultError> {
    let ctx = CommitContext::new(DEVICE_ID.to_string());
    EntryRepo::restore(conn, &ctx, entry_id).map_err(|error| match error {
        StorageError::NotFound(_) => VaultError::EntryNotFound(entry_id.to_string()),
        other => storage_error(other),
    })
}

pub(crate) fn list_by_type(
    conn: &VaultConnection,
    entry_type: EntryType,
) -> Result<Vec<Entry>, VaultError> {
    EntryRepo::list_by_type(conn, entry_type).map_err(storage_error)
}

pub(crate) fn list_deleted(conn: &VaultConnection) -> Result<Vec<Entry>, VaultError> {
    EntryRepo::list_deleted(conn).map_err(storage_error)
}
