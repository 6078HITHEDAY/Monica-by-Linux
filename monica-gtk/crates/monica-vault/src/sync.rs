//! Offline `mdbx-sync` complete bundles (`MDBXSYNC`).
//!
//! Live / cloud peer sync (WebDAV, OneDrive, Bitwarden, wire `SyncClient`) is
//! **not** wired. This module only exports and applies a complete file bundle
//! for the same `vault_id`. Incremental bundles need a persisted checkpoint
//! and are rejected with an explicit error.

use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use mdbx_storage::connection::VaultConnection;
use mdbx_storage::peer_sync::PeerSyncService;
use mdbx_storage::repo::CommitContext;
use mdbx_storage::sync_apply::SyncApplyRepo;
use mdbx_sync::{
    read_bundle_file_with_limits, write_bundle, BundleReadLimits, CommitBatch, SyncBundleFile,
};

use crate::{storage_error, VaultError, DEVICE_ID};

/// Honest status for the 同步 UI. No fake "last synced" timestamp.
pub const SYNC_STATUS_NOTE: &str = "可用：把完整 MDBXSYNC 包拷到另一台已解锁的同一密码库。不可用：WebDAV / OneDrive / Bitwarden 在线同步、实时对端、增量包（需 checkpoint）。";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncBundleInfo {
    pub path: PathBuf,
    pub vault_id: String,
    pub source_device_id: String,
    pub exported_at: String,
    pub commits: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncApplyInfo {
    pub path: PathBuf,
    pub vault_id: String,
    pub applied: u32,
    pub skipped: u32,
    pub conflicts: u32,
    pub missing_parents: u32,
}

pub(crate) fn export_complete_bundle(
    conn: &VaultConnection,
    destination: &Path,
) -> Result<SyncBundleInfo, VaultError> {
    if destination.exists() {
        return Err(VaultError::AlreadyExists(destination.to_path_buf()));
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| VaultError::Storage(error.to_string()))?;
    }
    let bundle = PeerSyncService::export_complete_bundle(conn, DEVICE_ID).map_err(storage_error)?;
    let info = SyncBundleInfo {
        path: destination.to_path_buf(),
        vault_id: bundle.vault_id.clone(),
        source_device_id: bundle.source_device_id.clone(),
        exported_at: bundle.exported_at.clone(),
        commits: bundle.commits.len() as u32,
    };
    let file = File::create(destination).map_err(|error| VaultError::Storage(error.to_string()))?;
    let mut writer = BufWriter::new(file);
    write_bundle(&bundle, &mut writer).map_err(storage_error)?;
    writer
        .flush()
        .map_err(|error| VaultError::Storage(error.to_string()))?;
    writer
        .get_ref()
        .sync_all()
        .map_err(|error| VaultError::Storage(error.to_string()))?;
    Ok(info)
}

pub(crate) fn apply_complete_bundle(
    conn: &mut VaultConnection,
    source: &Path,
) -> Result<SyncApplyInfo, VaultError> {
    let file = File::open(source).map_err(|error| VaultError::Storage(error.to_string()))?;
    let mut reader = BufReader::new(file);
    let bundle = read_bundle_file_with_limits(&mut reader, BundleReadLimits::desktop())
        .map_err(storage_error)?;
    match bundle {
        SyncBundleFile::Complete(bundle) => {
            let local_id: String = conn
                .inner()
                .query_row("SELECT vault_id FROM vault_meta LIMIT 1", [], |row| {
                    row.get(0)
                })
                .map_err(storage_error)?;
            if bundle.vault_id != local_id {
                return Err(VaultError::Storage(format!(
                    "同步包 vault_id {} 与当前密码库 {} 不一致，拒绝应用",
                    bundle.vault_id, local_id
                )));
            }
            let ctx = CommitContext::new(DEVICE_ID.to_string());
            let batch = CommitBatch::new(bundle.commits.clone(), 0, true);
            let result =
                SyncApplyRepo::apply_batch_mut(conn, &ctx, &batch).map_err(storage_error)?;
            Ok(SyncApplyInfo {
                path: source.to_path_buf(),
                vault_id: bundle.vault_id,
                applied: result.applied_commits,
                skipped: result.skipped_commits,
                conflicts: result.conflict_count,
                missing_parents: result.missing_parent_count,
            })
        }
        SyncBundleFile::Incremental(_) => Err(VaultError::Storage(
            "这是增量同步包，需要 checkpoint，当前界面只支持完整 MDBXSYNC 包".to_string(),
        )),
    }
}
