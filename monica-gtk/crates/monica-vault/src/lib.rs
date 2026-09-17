//! Thin vault adapter for the GTK4 Phase 0 spike.
//!
//! This crate depends on upstream [`mdbx-storage`] / [`mdbx-core`] from
//! [Monica-Pass/Mdbx](https://github.com/Monica-Pass/Mdbx) over git — not the
//! Avalonia UniFFI `libmdbx_ffi.so` bridge.
//!
//! Master-password material is wrapped in [`secrecy::SecretString`] so later
//! phases can audit `zeroize` / `secrecy` without changing the call shape.
//!
//! `VaultConnection::open` is writable and auto-upgrades `MDBX-1` /
//! `MDBX-1-DRAFT` (and stale `MDBX-2` schema) in place. Inspect and unlock
//! therefore go through the upstream read-only planner first
//! ([`mdbx_storage::migration::inspect_migration`]) and refuse any
//! in-place upgrade.

use std::path::{Path, PathBuf};
use std::time::Duration;

use mdbx_core::tiga::TigaMode;
use mdbx_storage::connection::{PendingVaultCreation, VaultConnection};
use mdbx_storage::init::{initialize_vault, VaultInitParams};
use mdbx_storage::migration::{self, MigrationInfo};
use mdbx_storage::unlock::UnlockService;
use rusqlite::{Connection, OpenFlags};
use secrecy::{ExposeSecret, SecretString};

const PHASE0_DEVICE_ID: &str = "monica-gtk-phase0";

/// Metadata that can be read after a storage-layer open (before or after unlock).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultInfo {
    pub path: PathBuf,
    pub vault_id: String,
    pub format_version: String,
    pub tiga_mode: String,
    pub schema_version: i64,
    pub unlocked: bool,
    /// True when a writable `VaultConnection::open` would migrate format or schema.
    pub requires_upgrade: bool,
}

#[derive(Debug)]
pub enum VaultError {
    AlreadyExists(PathBuf),
    NotFound(PathBuf),
    /// Writable open would upgrade this file in place. Copy/backup first.
    UpgradeRequired {
        path: PathBuf,
        format_version: String,
        target_format_version: String,
    },
    Storage(String),
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyExists(path) => {
                write!(f, "vault already exists: {}", path.display())
            }
            Self::NotFound(path) => write!(f, "vault not found: {}", path.display()),
            Self::UpgradeRequired {
                path,
                format_version,
                target_format_version,
            } => write!(
                f,
                "保险库 {} 当前为 {format_version}，原地打开会升级为 {target_format_version}。请先复制或备份该文件，再对副本使用可写客户端。",
                path.display()
            ),
            Self::Storage(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for VaultError {}

/// Wrap UI / CLI password bytes as a secret. Callers should drop the source
/// `String` immediately after this conversion.
pub fn secret_password(password: String) -> SecretString {
    SecretString::from(password)
}

/// Create a new `.mdbx` vault and configure password unlock.
///
/// Uses Tiga Sky so Phase 0 tests stay cheap (8 MiB Argon2id). Production
/// clients should pick Multi/Power explicitly.
pub fn create_vault(path: &Path, password: &SecretString) -> Result<VaultInfo, VaultError> {
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
        device_id: PHASE0_DEVICE_ID.to_string(),
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
    read_info(path, &connection, true, false)
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
pub fn unlock_vault(path: &Path, password: &SecretString) -> Result<VaultInfo, VaultError> {
    let (_readonly, migration) = inspect_existing(path)?;
    refuse_in_place_upgrade(path, &migration)?;
    drop(_readonly);

    let mut connection =
        VaultConnection::open(path).map_err(|error| VaultError::Storage(error.to_string()))?;
    UnlockService::unlock_with_password(&mut connection, password.expose_secret())
        .map_err(|error| VaultError::Storage(error.to_string()))?;
    read_info(path, &connection, true, false)
}

/// Create a temp vault, reopen it, unlock with the right password, and reject
/// a wrong password. Used by `monica-gtk --self-test` and unit tests.
pub fn self_test() -> Result<String, VaultError> {
    let directory = tempfile::tempdir().map_err(|error| VaultError::Storage(error.to_string()))?;
    let path = directory.path().join("local.mdbx");
    let password = secret_password("phase0-test-password".to_string());

    let created = create_vault(&path, &password)?;
    let before_inspect = file_fingerprint(&path)?;
    let inspected = inspect_vault(&path)?;
    let after_inspect = file_fingerprint(&path)?;
    if before_inspect != after_inspect {
        return Err(VaultError::Storage(
            "inspect_vault mutated the vault file".to_string(),
        ));
    }
    let unlocked = unlock_vault(&path, &password)?;
    let rejected = unlock_vault(&path, &secret_password("wrong-password".to_string()));

    if inspected.vault_id != created.vault_id {
        return Err(VaultError::Storage(
            "reopened vault_id did not match the created vault".to_string(),
        ));
    }
    if inspected.requires_upgrade {
        return Err(VaultError::Storage(
            "self-created vault unexpectedly requires upgrade".to_string(),
        ));
    }
    if !unlocked.unlocked {
        return Err(VaultError::Storage(
            "unlock did not attach a session".to_string(),
        ));
    }
    if rejected.is_ok() {
        return Err(VaultError::Storage(
            "unlock with the wrong password unexpectedly succeeded".to_string(),
        ));
    }

    Ok(format!(
        "mdbx-storage git crate opened local.mdbx: vault_id={} format={} schema={} tiga={} unlocked={}",
        unlocked.vault_id,
        unlocked.format_version,
        unlocked.schema_version,
        unlocked.tiga_mode,
        unlocked.unlocked
    ))
}

fn inspect_existing(path: &Path) -> Result<(Connection, MigrationInfo), VaultError> {
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

fn file_fingerprint(path: &Path) -> Result<(u64, Vec<u8>), VaultError> {
    let metadata =
        std::fs::metadata(path).map_err(|error| VaultError::Storage(error.to_string()))?;
    let bytes = std::fs::read(path).map_err(|error| VaultError::Storage(error.to_string()))?;
    Ok((metadata.len(), bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_vault_path(name: &str) -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join(name);
        (directory, path)
    }

    fn write_legacy_mdbx1_stub(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        let connection = Connection::open(path).expect("create stub vault");
        connection
            .execute_batch(
                "CREATE TABLE vault_meta (
                    vault_id TEXT NOT NULL,
                    format_version TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    default_tiga_mode TEXT NOT NULL,
                    active_key_epoch_id TEXT NOT NULL,
                    compat_flags TEXT NOT NULL,
                    critical_extensions TEXT NOT NULL
                );
                INSERT INTO vault_meta (
                    vault_id, format_version, created_at, updated_at,
                    default_tiga_mode, active_key_epoch_id, compat_flags, critical_extensions
                ) VALUES (
                    'legacy-avalonia-vault', 'MDBX-1', '2026-01-01T00:00:00Z',
                    '2026-01-01T00:00:00Z', 'multi', 'epoch-1', '', ''
                );",
            )
            .expect("insert MDBX-1 stub");
    }

    fn readonly_format_version(path: &Path) -> String {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("read-only reopen");
        connection
            .query_row("SELECT format_version FROM vault_meta LIMIT 1", [], |row| {
                row.get(0)
            })
            .expect("format_version")
    }

    #[test]
    fn creates_reopens_and_unlocks_local_mdbx() {
        let summary = self_test().expect("phase 0 vault round-trip");
        assert!(summary.contains("format="), "{summary}");
        assert!(summary.contains("unlocked=true"), "{summary}");
    }

    #[test]
    fn inspect_does_not_mutate_current_format_vault() {
        let (_directory, path) = temp_vault_path("current.mdbx");
        let password = secret_password("phase0-test-password".to_string());
        let created = create_vault(&path, &password).expect("create");
        let before = file_fingerprint(&path).expect("fingerprint before");
        let wal_before = wal_sidecar_exists(&path);

        let inspected = inspect_vault(&path).expect("inspect current vault");

        assert_eq!(inspected.vault_id, created.vault_id);
        assert_eq!(inspected.format_version, created.format_version);
        assert!(!inspected.unlocked);
        assert!(!inspected.requires_upgrade);
        assert_eq!(
            file_fingerprint(&path).expect("fingerprint after"),
            before,
            "inspect_vault must not rewrite a current-format vault"
        );
        assert_eq!(
            wal_sidecar_exists(&path),
            wal_before,
            "read-only inspect must not create or drop a WAL sidecar"
        );
    }

    #[test]
    fn inspect_does_not_upgrade_legacy_mdbx1_file() {
        let (_directory, path) = temp_vault_path("legacy.mdbx");
        write_legacy_mdbx1_stub(&path);
        let before = file_fingerprint(&path).expect("fingerprint before");

        let inspected = inspect_vault(&path).expect("inspect MDBX-1 stub");

        assert_eq!(inspected.vault_id, "legacy-avalonia-vault");
        assert_eq!(inspected.format_version, "MDBX-1");
        assert!(inspected.requires_upgrade);
        assert!(!inspected.unlocked);
        assert_eq!(readonly_format_version(&path), "MDBX-1");
        assert_eq!(
            file_fingerprint(&path).expect("fingerprint after"),
            before,
            "inspect_vault must not upgrade MDBX-1 in place"
        );
        assert!(
            !wal_sidecar_exists(&path),
            "read-only inspect must not create a WAL sidecar on a legacy vault"
        );
    }

    #[test]
    fn unlock_refuses_legacy_mdbx1_without_upgrading() {
        let (_directory, path) = temp_vault_path("legacy-unlock.mdbx");
        write_legacy_mdbx1_stub(&path);
        let before = file_fingerprint(&path).expect("fingerprint before");
        let password = secret_password("any-password".to_string());

        let error = unlock_vault(&path, &password).expect_err("MDBX-1 unlock must refuse");

        assert!(
            matches!(
                error,
                VaultError::UpgradeRequired {
                    ref format_version,
                    ref target_format_version,
                    ..
                } if format_version == "MDBX-1" && target_format_version == "MDBX-2"
            ),
            "unexpected error: {error}"
        );
        assert_eq!(readonly_format_version(&path), "MDBX-1");
        assert_eq!(
            file_fingerprint(&path).expect("fingerprint after"),
            before,
            "refused unlock must not mutate a legacy vault"
        );
    }

    #[test]
    fn unlock_of_current_format_does_not_change_format_version() {
        let (_directory, path) = temp_vault_path("unlock-current.mdbx");
        let password = secret_password("phase0-test-password".to_string());
        let created = create_vault(&path, &password).expect("create");

        let unlocked = unlock_vault(&path, &password).expect("unlock current vault");
        assert!(unlocked.unlocked);
        assert_eq!(unlocked.format_version, created.format_version);
        assert_eq!(readonly_format_version(&path), created.format_version);

        let rejected = unlock_vault(&path, &secret_password("wrong-password".to_string()));
        assert!(rejected.is_err(), "wrong password should fail");
        assert_eq!(readonly_format_version(&path), created.format_version);
    }

    fn wal_sidecar_exists(path: &Path) -> bool {
        let mut wal = path.as_os_str().to_os_string();
        wal.push("-wal");
        PathBuf::from(wal).exists()
    }
}
