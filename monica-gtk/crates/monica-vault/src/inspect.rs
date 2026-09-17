//! Read-only inspect, vault creation, and one-shot unlock helpers (Phase 0).
//!
//! `VaultConnection::open` is writable and auto-upgrades `MDBX-1` /
//! `MDBX-1-DRAFT` (and stale `MDBX-2` schema) in place. Inspect and unlock
//! therefore go through the upstream read-only planner first
//! ([`mdbx_storage::migration::inspect_migration`]) and refuse any
//! in-place upgrade.

use std::path::Path;
use std::time::Duration;

use mdbx_core::tiga::TigaMode;
use mdbx_storage::connection::{PendingVaultCreation, VaultConnection};
use mdbx_storage::init::{initialize_vault, VaultInitParams};
use mdbx_storage::migration::{self, MigrationInfo};
use mdbx_storage::unlock::UnlockService;
use rusqlite::{Connection, OpenFlags};
use secrecy::{ExposeSecret, SecretString};

use crate::{VaultError, VaultInfo, DEVICE_ID};

/// Create a new `.mdbx` vault and configure password unlock.
///
/// Uses Tiga Sky so tests stay cheap (8 MiB Argon2id). Production
/// clients should pick Multi/Power explicitly.
///
/// The returned [`VaultInfo`] describes a vault that was unlocked during
/// setup; the connection is not kept. Use [`crate::create_session`] to keep
/// an authenticated session for entry I/O.
pub fn create_vault(path: &Path, password: &SecretString) -> Result<VaultInfo, VaultError> {
    Ok(create_unlocked_connection(path, password)?.1)
}

/// Inspect an existing vault through a **read-only** SQLite open.
///
/// Uses upstream [`migration::inspect_migration`] on a `SQLITE_OPEN_READ_ONLY`
/// handle (URI `mode=ro&immutable=1` so WAL-mode files do not grow sidecars).
/// This will not auto-upgrade `MDBX-1` / `MDBX-1-DRAFT`.
pub fn inspect_vault(path: &Path) -> Result<VaultInfo, VaultError> {
    let (connection, migration) = inspect_existing(path)?;
    read_info_readonly(path, &connection, &migration)
}

/// Open and unlock with a master password. The secret is only exposed at the
/// `mdbx-storage` call boundary (same shape as later `zeroize` work).
///
/// Upstream has no `VaultConnection::open_readonly`. Password attempts therefore
/// plan with the read-only inspector first and **refuse** any file that would
/// be upgraded in place. Current-format vaults still use writable
/// `VaultConnection::open` (WAL / `secure_delete` pragmas) after that gate.
///
/// The connection is dropped after the metadata read. Use
/// [`crate::unlock_session`] to keep the session.
pub fn unlock_vault(path: &Path, password: &SecretString) -> Result<VaultInfo, VaultError> {
    Ok(unlock_connection(path, password)?.1)
}

pub(crate) fn create_unlocked_connection(
    path: &Path,
    password: &SecretString,
) -> Result<(VaultConnection, VaultInfo), VaultError> {
    if path.exists() {
        return Err(VaultError::AlreadyExists(path.to_path_buf()));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| VaultError::Storage(error.to_string()))?;
    }

    let mut creation = PendingVaultCreation::begin(path)
        .map_err(|error| VaultError::Storage(error.to_string()))?;
    let params = VaultInitParams {
        default_tiga_mode: TigaMode::Sky.to_string(),
        device_id: DEVICE_ID.to_string(),
        ..VaultInitParams::default()
    };
    initialize_vault(creation.connection(), &params)
        .map_err(|error| VaultError::Storage(error.to_string()))?;
    UnlockService::setup_password_with_mode(
        creation.connection_mut(),
        password.expose_secret(),
        TigaMode::Sky,
    )
    .map_err(|error| VaultError::Storage(error.to_string()))?;

    let connection = creation.commit();
    let info = read_info(path, &connection, true, false)?;
    Ok((connection, info))
}

pub(crate) fn unlock_connection(
    path: &Path,
    password: &SecretString,
) -> Result<(VaultConnection, VaultInfo), VaultError> {
    let (_readonly, migration) = inspect_existing(path)?;
    refuse_in_place_upgrade(path, &migration)?;
    drop(_readonly);

    let mut connection =
        VaultConnection::open(path).map_err(|error| VaultError::Storage(error.to_string()))?;
    UnlockService::unlock_with_password(&mut connection, password.expose_secret())
        .map_err(|error| VaultError::Storage(error.to_string()))?;
    let info = read_info(path, &connection, true, false)?;
    Ok((connection, info))
}

pub(crate) fn inspect_existing(path: &Path) -> Result<(Connection, MigrationInfo), VaultError> {
    if !path.exists() {
        return Err(VaultError::NotFound(path.to_path_buf()));
    }
    let connection = open_readonly(path)?;
    let migration = migration::inspect_migration(&connection)
        .map_err(|error| VaultError::Storage(error.to_string()))?;
    if !migration.initialized {
        return Err(VaultError::Storage(format!(
            "not an initialized MDBX vault: {}",
            path.display()
        )));
    }
    Ok((connection, migration))
}

fn refuse_in_place_upgrade(path: &Path, migration: &MigrationInfo) -> Result<(), VaultError> {
    if !migration.requires_upgrade {
        return Ok(());
    }
    Err(VaultError::UpgradeRequired {
        path: path.to_path_buf(),
        format_version: migration
            .format_version
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        target_format_version: migration.target_format_version.clone(),
    })
}

fn sqlite_readonly_uri(path: &Path) -> String {
    let mut uri = String::from("file:");
    for byte in path.as_os_str().as_encoded_bytes() {
        match *byte {
            b'/' | b'-' | b'_' | b'.' | b'~' | b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => {
                uri.push(char::from(*byte));
            }
            other => uri.push_str(&format!("%{other:02X}")),
        }
    }
    uri.push_str("?mode=ro&immutable=1");
    uri
}

fn open_readonly(path: &Path) -> Result<Connection, VaultError> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI;
    let connection = Connection::open_with_flags(sqlite_readonly_uri(path), flags)
        .or_else(|_| Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY))
        .map_err(|error| VaultError::Storage(error.to_string()))?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| VaultError::Storage(error.to_string()))?;
    Ok(connection)
}

fn read_info_readonly(
    path: &Path,
    connection: &Connection,
    migration: &MigrationInfo,
) -> Result<VaultInfo, VaultError> {
    let vault_id: String = connection
        .query_row("SELECT vault_id FROM vault_meta LIMIT 1", [], |row| {
            row.get(0)
        })
        .map_err(|error| VaultError::Storage(error.to_string()))?;
    let tiga_mode = connection
        .query_row(
            "SELECT default_tiga_mode FROM vault_meta LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_else(|_| "unknown".to_string());

    Ok(VaultInfo {
        path: path.to_path_buf(),
        vault_id,
        format_version: migration
            .format_version
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        tiga_mode,
        schema_version: i64::from(migration.schema_version.unwrap_or(0)),
        unlocked: false,
        requires_upgrade: migration.requires_upgrade,
    })
}

fn read_info(
    path: &Path,
    connection: &VaultConnection,
    unlocked: bool,
    requires_upgrade: bool,
) -> Result<VaultInfo, VaultError> {
    let (vault_id, format_version, tiga_mode, schema_version): (String, String, String, i64) =
        connection
            .inner()
            .query_row(
                "SELECT vault_id, format_version, default_tiga_mode, schema_version
                 FROM vault_meta LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(|error| VaultError::Storage(error.to_string()))?;

    Ok(VaultInfo {
        path: path.to_path_buf(),
        vault_id,
        format_version,
        tiga_mode,
        schema_version,
        unlocked,
        requires_upgrade,
    })
}

pub(crate) fn file_fingerprint(path: &Path) -> Result<(u64, Vec<u8>), VaultError> {
    let metadata =
        std::fs::metadata(path).map_err(|error| VaultError::Storage(error.to_string()))?;
    let bytes = std::fs::read(path).map_err(|error| VaultError::Storage(error.to_string()))?;
    Ok((metadata.len(), bytes))
}
