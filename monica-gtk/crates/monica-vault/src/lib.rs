//! Thin vault adapter for the GTK4 Phase 0 spike.
//!
//! This crate depends on upstream [`mdbx-storage`] / [`mdbx-core`] from
//! [Monica-Pass/Mdbx](https://github.com/Monica-Pass/Mdbx) over git — not the
//! Avalonia UniFFI `libmdbx_ffi.so` bridge.
//!
//! Master-password material is wrapped in [`secrecy::SecretString`] so later
//! phases can audit `zeroize` / `secrecy` without changing the call shape.

use std::path::{Path, PathBuf};

use mdbx_core::tiga::TigaMode;
use mdbx_storage::connection::{PendingVaultCreation, VaultConnection};
use mdbx_storage::init::{initialize_vault, VaultInitParams};
use mdbx_storage::unlock::UnlockService;
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
}

#[derive(Debug)]
pub enum VaultError {
    AlreadyExists(PathBuf),
    NotFound(PathBuf),
    Storage(String),
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyExists(path) => {
                write!(f, "vault already exists: {}", path.display())
            }
            Self::NotFound(path) => write!(f, "vault not found: {}", path.display()),
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
    read_info(path, &connection, true)
}

/// Open an existing vault file through `mdbx-storage`.
///
/// **Upgrade warning:** `VaultConnection::open` is a writable open and will
/// auto-upgrade `MDBX-1` / `MDBX-1-DRAFT` to `MDBX-2`. Do not point this at a
/// live Avalonia `local.mdbx` without a copy.
pub fn inspect_vault(path: &Path) -> Result<VaultInfo, VaultError> {
    if !path.exists() {
        return Err(VaultError::NotFound(path.to_path_buf()));
    }
    let connection =
        VaultConnection::open(path).map_err(|error| VaultError::Storage(error.to_string()))?;
    read_info(path, &connection, false)
}

/// Open and unlock with a master password. The secret is only exposed at the
/// `mdbx-storage` call boundary (same shape as later `zeroize` work).
pub fn unlock_vault(path: &Path, password: &SecretString) -> Result<VaultInfo, VaultError> {
    if !path.exists() {
        return Err(VaultError::NotFound(path.to_path_buf()));
    }
    let mut connection =
        VaultConnection::open(path).map_err(|error| VaultError::Storage(error.to_string()))?;
    UnlockService::unlock_with_password(&mut connection, password.expose_secret())
        .map_err(|error| VaultError::Storage(error.to_string()))?;
    read_info(path, &connection, true)
}

/// Create a temp vault, reopen it, unlock with the right password, and reject
/// a wrong password. Used by `monica-gtk --self-test` and unit tests.
pub fn self_test() -> Result<String, VaultError> {
    let directory = tempfile::tempdir().map_err(|error| VaultError::Storage(error.to_string()))?;
    let path = directory.path().join("local.mdbx");
    let password = secret_password("phase0-test-password".to_string());

    let created = create_vault(&path, &password)?;
    let inspected = inspect_vault(&path)?;
    let unlocked = unlock_vault(&path, &password)?;
    let rejected = unlock_vault(&path, &secret_password("wrong-password".to_string()));

    if inspected.vault_id != created.vault_id {
        return Err(VaultError::Storage(
            "reopened vault_id did not match the created vault".to_string(),
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

fn read_info(
    path: &Path,
    connection: &VaultConnection,
    unlocked: bool,
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_reopens_and_unlocks_local_mdbx() {
        let summary = self_test().expect("phase 0 vault round-trip");
        assert!(summary.contains("format="), "{summary}");
        assert!(summary.contains("unlocked=true"), "{summary}");
    }
}
