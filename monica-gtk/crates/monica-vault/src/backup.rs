//! Portable `.mdbx` vault copies via upstream [`mdbx_storage::backup::BackupService`].
//!
//! Format (same as `mdbx-cli backup` / UniFFI `create_portable_backup`):
//! - SQLite online backup into a new file (`VACUUM`-equivalent page copy).
//! - Destination journal is `DELETE`; WAL/SHM sidecars are not published.
//! - Ciphertext only. No unlock and no format migration.
//! - Source generation is preserved (`MDBX-1` stays `MDBX-1`).
//! - Destination and its sidecars must not already exist.

use std::path::{Path, PathBuf};

use mdbx_storage::backup::{BackupService, VaultBackupInfo as StorageBackupInfo};
use mdbx_storage::connection::VaultConnection;

use crate::{storage_error, VaultError};

/// Published portable copy. The file at [`Self::path`] is a self-contained vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultBackupInfo {
    pub path: PathBuf,
    pub vault_id: String,
    pub format_version: String,
    pub schema_version: i64,
    pub file_size_bytes: u64,
}

impl VaultBackupInfo {
    fn from_storage(path: PathBuf, info: StorageBackupInfo) -> Self {
        Self {
            path,
            vault_id: info.vault_id,
            format_version: info.format_version,
            schema_version: i64::from(info.schema_version),
            file_size_bytes: info.file_size_bytes,
        }
    }
}

/// Copy `source` to `destination` without a writable open or unlock.
pub fn backup_vault_file(source: &Path, destination: &Path) -> Result<VaultBackupInfo, VaultError> {
    let info = BackupService::create_portable_copy_path(source, destination).map_err(storage_error)?;
    Ok(VaultBackupInfo::from_storage(
        destination.to_path_buf(),
        info,
    ))
}

pub(crate) fn backup_live_connection(
    conn: &VaultConnection,
    destination: &Path,
) -> Result<VaultBackupInfo, VaultError> {
    let info = BackupService::create_portable_copy(conn, destination).map_err(storage_error)?;
    Ok(VaultBackupInfo::from_storage(
        destination.to_path_buf(),
        info,
    ))
}
