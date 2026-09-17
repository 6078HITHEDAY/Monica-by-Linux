//! Thin vault adapter for the GTK4 Linux client.
//!
//! This crate depends on upstream [`mdbx-storage`] / [`mdbx-core`] from
//! [Monica-Pass/Mdbx](https://github.com/Monica-Pass/Mdbx) over git — not the
//! Avalonia UniFFI `libmdbx_ffi.so` bridge.
//!
//! Master-password and login-password material is wrapped in
//! [`secrecy::SecretString`]. Authenticated I/O goes through [`VaultSession`],
//! which owns upstream [`mdbx_storage::runtime::VaultRuntime`] and clears the
//! keyring on lock / drop.

use std::path::PathBuf;

use secrecy::SecretString;

mod inspect;
mod password;
mod session;

pub use inspect::{create_vault, inspect_vault, unlock_vault};
pub use password::{PasswordEntryDetail, PasswordEntryDraft, PasswordEntrySummary};
pub use session::{create_session, unlock_session, VaultSession};

pub(crate) const DEVICE_ID: &str = "monica-gtk-phase1";

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
    EntryNotFound(String),
    /// The authenticated session was locked or dropped.
    Locked,
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
            Self::EntryNotFound(entry_id) => write!(f, "password entry not found: {entry_id}"),
            Self::Locked => write!(f, "保险库已锁定"),
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

pub(crate) fn storage_error(error: impl ToString) -> VaultError {
    VaultError::Storage(error.to_string())
}

/// Wrap UI / CLI password bytes as a secret. Callers should drop the source
/// `String` immediately after this conversion.
pub fn secret_password(password: String) -> SecretString {
    SecretString::from(password)
}

/// Create a temp vault, reopen it, unlock with the right password, reject a
/// wrong password, then exercise login CRUD, soft-delete, and session lock.
/// Used by `monica-gtk --self-test` and unit tests.
pub fn self_test() -> Result<String, VaultError> {
    let directory = tempfile::tempdir().map_err(|error| VaultError::Storage(error.to_string()))?;
    let path = directory.path().join("local.mdbx");
    let password = secret_password("phase0-test-password".to_string());

    let created = create_vault(&path, &password)?;
    let before_inspect = inspect::file_fingerprint(&path)?;
    let inspected = inspect_vault(&path)?;
    let after_inspect = inspect::file_fingerprint(&path)?;
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

    let crud = session_crud_self_test(&path, &password)?;

    Ok(format!(
        "mdbx-storage git crate opened local.mdbx: vault_id={} format={} schema={} tiga={} unlocked={} {crud}",
        unlocked.vault_id,
        unlocked.format_version,
        unlocked.schema_version,
        unlocked.tiga_mode,
        unlocked.unlocked
    ))
}

fn session_crud_self_test(
    path: &std::path::Path,
    password: &SecretString,
) -> Result<String, VaultError> {
    use secrecy::ExposeSecret;

    let session = unlock_session(path, password)?;
    if !session.list_password_entries()?.is_empty() {
        return Err(VaultError::Storage(
            "new vault already contained login entries".to_string(),
        ));
    }

    let created = session.save_password_entry(&PasswordEntryDraft {
        entry_id: None,
        title: "GitHub".into(),
        username: "ada".into(),
        url: "https://github.com".into(),
        notes: "main account".into(),
        password: secret_password("s3cret".into()),
    })?;
    let listed = session.list_password_entries()?;
    if listed.len() != 1
        || listed[0].title != "GitHub"
        || listed[0].username != "ada"
        || listed[0].url != "https://github.com"
    {
        return Err(VaultError::Storage(format!(
            "list after create mismatch: {listed:?}"
        )));
    }

    let detail = session.get_password_entry(&created.entry_id)?;
    if detail.password.expose_secret() != "s3cret" || detail.notes != "main account" {
        return Err(VaultError::Storage(
            "detail did not return the saved secret".to_string(),
        ));
    }

    let updated = session.save_password_entry(&PasswordEntryDraft {
        entry_id: Some(created.entry_id.clone()),
        title: "GitHub".into(),
        username: "ada-lovelace".into(),
        url: "https://github.com".into(),
        notes: "rotated".into(),
        password: secret_password("n3w-secret".into()),
    })?;
    let detail = session.get_password_entry(&updated.entry_id)?;
    if detail.username != "ada-lovelace" || detail.password.expose_secret() != "n3w-secret" {
        return Err(VaultError::Storage("edit did not persist".to_string()));
    }

    session.delete_password_entry(&updated.entry_id)?;
    if !session.list_password_entries()?.is_empty() {
        return Err(VaultError::Storage(
            "soft-delete left the login visible".to_string(),
        ));
    }

    session.lock();
    let locked = session.list_password_entries();
    if !matches!(locked, Err(VaultError::Locked)) {
        return Err(VaultError::Storage(format!(
            "list after lock should fail, got {locked:?}"
        )));
    }

    Ok(format!("crud=ok entries_after_delete=0 lock=ok"))
}

#[cfg(test)]
mod tests {
    use rusqlite::{Connection, OpenFlags};

    use super::*;
    use std::path::{Path, PathBuf};

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

    fn wal_sidecar_exists(path: &Path) -> bool {
        let mut wal = path.as_os_str().to_os_string();
        wal.push("-wal");
        PathBuf::from(wal).exists()
    }

    #[test]
    fn creates_reopens_and_unlocks_local_mdbx() {
        let summary = self_test().expect("phase 1 vault round-trip");
        assert!(summary.contains("format="), "{summary}");
        assert!(summary.contains("unlocked=true"), "{summary}");
        assert!(summary.contains("crud=ok"), "{summary}");
        assert!(summary.contains("lock=ok"), "{summary}");
    }

    #[test]
    fn inspect_does_not_mutate_current_format_vault() {
        let (_directory, path) = temp_vault_path("current.mdbx");
        let password = secret_password("phase0-test-password".to_string());
        let created = create_vault(&path, &password).expect("create");
        let before = inspect::file_fingerprint(&path).expect("fingerprint before");
        let wal_before = wal_sidecar_exists(&path);

        let inspected = inspect_vault(&path).expect("inspect current vault");

        assert_eq!(inspected.vault_id, created.vault_id);
        assert_eq!(inspected.format_version, created.format_version);
        assert!(!inspected.unlocked);
        assert!(!inspected.requires_upgrade);
        assert_eq!(
            inspect::file_fingerprint(&path).expect("fingerprint after"),
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
        let before = inspect::file_fingerprint(&path).expect("fingerprint before");

        let inspected = inspect_vault(&path).expect("inspect MDBX-1 stub");

        assert_eq!(inspected.vault_id, "legacy-avalonia-vault");
        assert_eq!(inspected.format_version, "MDBX-1");
        assert!(inspected.requires_upgrade);
        assert!(!inspected.unlocked);
        assert_eq!(readonly_format_version(&path), "MDBX-1");
        assert_eq!(
            inspect::file_fingerprint(&path).expect("fingerprint after"),
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
        let before = inspect::file_fingerprint(&path).expect("fingerprint before");
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
            inspect::file_fingerprint(&path).expect("fingerprint after"),
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

    #[test]
    fn session_soft_delete_hides_login_from_list() {
        let (_directory, path) = temp_vault_path("crud.mdbx");
        let password = secret_password("phase1-crud".to_string());
        let session = create_session(&path, &password).expect("create session");
        let saved = session
            .save_password_entry(&PasswordEntryDraft {
                entry_id: None,
                title: "Mail".into(),
                username: "ada".into(),
                url: "https://mail.example".into(),
                notes: String::new(),
                password: secret_password("mailbox".into()),
            })
            .expect("save");
        session
            .delete_password_entry(&saved.entry_id)
            .expect("delete");
        assert!(session.list_password_entries().expect("list").is_empty());
        assert!(matches!(
            session.get_password_entry(&saved.entry_id),
            Err(VaultError::EntryNotFound(_))
        ));
    }
}
