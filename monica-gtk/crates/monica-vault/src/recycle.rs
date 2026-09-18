//! Recycle bin: list/restore soft-deleted entries and authorized permanent purge.

use chrono::{DateTime, Utc};
use mdbx_core::tiga::{DeviceAssurance, DeviceContext};
use mdbx_storage::connection::VaultConnection;
use mdbx_storage::error::StorageError;
use mdbx_storage::repo::{CommitContext, TombstoneRepo};
use mdbx_storage::tiga_policy::TigaAuthorizationContext;

use crate::io::{list_deleted, restore_entry as restore_loaded};
use crate::payload::{kind_label, take_payload_json, title_from_bytes};
use crate::{storage_error, VaultError, DEVICE_ID};

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

/// Permanent purge via upstream `schedule_purge_authorized` + `purge_authorized`.
///
/// Mirrors Android FFI `schedule_tombstone_purge` / `purge_tombstone`: Sky-mode
/// local vaults use the unlocked connection session and a Standard device
/// context. Retention is scheduled at `deleted_at` (or now if later) so a
/// single-device vault can purge promptly. Soft-delete already writes the
/// deleting device's tombstone acknowledgement.
pub(crate) fn purge_trash_item(conn: &VaultConnection, entry_id: &str) -> Result<(), VaultError> {
    let tombstone = TombstoneRepo::find_by_target(conn, entry_id)
        .map_err(storage_error)?
        .ok_or_else(|| VaultError::EntryNotFound(entry_id.to_string()))?;
    let session = conn.active_session().cloned().ok_or_else(|| {
        VaultError::Storage("请重新解锁后再永久删除".to_string())
    })?;
    let device = DeviceContext {
        device_id: Some(DEVICE_ID.to_string()),
        assurance: DeviceAssurance::Standard,
        secure_clipboard_available: false,
        screen_capture_protection_available: false,
        secure_temp_files_available: true,
    };
    let (eligible_at, now_unix_secs) = purge_schedule_clock(&tombstone.deleted_at)?;
    let ctx = CommitContext::new(DEVICE_ID.to_string());
    let context = TigaAuthorizationContext {
        session: Some(&session),
        device: &device,
        now_unix_secs,
    };
    TombstoneRepo::schedule_purge_authorized(
        conn,
        &ctx,
        &tombstone.tombstone_id,
        &eligible_at,
        context,
    )
    .map_err(map_purge_error)?;
    TombstoneRepo::purge_authorized(conn, &ctx, &tombstone.tombstone_id, context)
        .map_err(map_purge_error)?;
    Ok(())
}

fn purge_schedule_clock(deleted_at: &str) -> Result<(String, i64), VaultError> {
    let deleted = DateTime::parse_from_rfc3339(deleted_at)
        .map_err(|error| VaultError::Storage(format!("墓碑时间无效：{error}")))?
        .with_timezone(&Utc);
    let now_unix = Utc::now().timestamp();
    let mut eligible_secs = deleted.timestamp().max(now_unix);
    let eligible = DateTime::<Utc>::from_timestamp(eligible_secs, 0).ok_or_else(|| {
        VaultError::Storage("时间无效".to_string())
    })?;
    if eligible < deleted {
        eligible_secs += 1;
    }
    let eligible_at = DateTime::<Utc>::from_timestamp(eligible_secs, 0)
        .ok_or_else(|| VaultError::Storage("时间无效".to_string()))?
        .to_rfc3339();
    Ok((eligible_at, eligible_secs.max(now_unix)))
}

fn map_purge_error(error: StorageError) -> VaultError {
    match error {
        StorageError::Authorization(decision) => VaultError::Storage(match decision.outcome {
            mdbx_core::tiga::AuthorizationOutcome::RequireFreshAuthentication => {
                "请重新解锁后再永久删除".to_string()
            }
            mdbx_core::tiga::AuthorizationOutcome::RequireAdditionalFactor => {
                "需要更多认证因素".to_string()
            }
            mdbx_core::tiga::AuthorizationOutcome::Deny => "无权永久删除".to_string(),
            _ => format!("永久删除未获授权：{:?}", decision.outcome),
        }),
        StorageError::ConstraintViolation(message) if message.contains("not eligible") => {
            VaultError::Storage("尚未可永久删除（保留期或设备未确认）".to_string())
        }
        StorageError::ConstraintViolation(message) if message.contains("dependent") => {
            VaultError::Storage("仍有关联对象，无法永久删除".to_string())
        }
        other => VaultError::Storage(format!("永久删除失败：{other}")),
    }
}
