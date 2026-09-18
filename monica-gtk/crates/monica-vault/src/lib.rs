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

mod archive;
mod backup;
mod csv;
mod exchange;
mod generator;
mod inspect;
mod io;
mod note;
mod password;
mod payload;
mod project;
mod recycle;
mod session;
mod sync;
mod timeline;
mod totp;
mod wallet;
mod workbench;

pub use archive::ArchivedItem;
pub use backup::{backup_vault_file, VaultBackupInfo};
pub use csv::{COMBINED_CSV_FORMAT, PASSWORD_CSV_FORMAT};
pub use exchange::{TransferSummary, MONICA_JSON_FORMAT};
pub use generator::{analyze_password, generate_password, GeneratorOptions, PasswordStrength};
pub use inspect::{create_vault, inspect_vault, unlock_vault};
pub use note::{NoteDetail, NoteDraft, NoteSummary};
pub use password::{PasswordEntryDetail, PasswordEntryDraft, PasswordEntrySummary};
pub use recycle::{permanent_delete_blocked, TrashItem, PERMANENT_DELETE_BLOCKED};
pub use session::{create_session, unlock_session, VaultSession};
pub use sync::{SyncApplyInfo, SyncBundleInfo, SYNC_STATUS_NOTE};
pub use timeline::TimelineItem;
pub use totp::{totp_at, totp_now, TotpCode, TotpDetail, TotpDraft, TotpSource, TotpSummary};
pub use wallet::{mask_digits, WalletDetail, WalletDraft, WalletKind, WalletSummary};
pub use workbench::{inspect_workbench, WorkbenchSnapshot};

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
            Self::EntryNotFound(entry_id) => write!(f, "条目不存在: {entry_id}"),
            Self::Locked => write!(f, "密码库已锁定"),
            Self::UpgradeRequired {
                path,
                format_version,
                target_format_version,
            } => write!(
                f,
                "密码库 {} 当前为 {format_version}，原地打开会升级为 {target_format_version}。请先复制或备份该文件，再对副本使用可写客户端。",
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
/// wrong password, then exercise Phase 2 entry flows and session lock.
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

    let generated = generate_password(GeneratorOptions::default());
    if generated.expose_secret().len() != 20 {
        return Err(VaultError::Storage("generator length mismatch".to_string()));
    }

    let created = session.save_password_entry(&PasswordEntryDraft {
        entry_id: None,
        title: "GitHub".into(),
        username: "ada".into(),
        url: "https://github.com".into(),
        notes: "main account".into(),
        password: secret_password("s3cret".into()),
        totp_secret: secret_password("JBSWY3DPEHPK3PXP".into()),
        archived: false,
    })?;
    let listed = session.list_password_entries()?;
    if listed.len() != 1
        || listed[0].title != "GitHub"
        || listed[0].username != "ada"
        || listed[0].url != "https://github.com"
        || !listed[0].has_totp
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
    let code = totp_now(detail.totp_secret.expose_secret(), 30, 6);
    if code.code.len() != 6 || code.period != 30 {
        return Err(VaultError::Storage(format!("login totp code: {code:?}")));
    }

    let updated = session.save_password_entry(&PasswordEntryDraft {
        entry_id: Some(created.entry_id.clone()),
        title: "GitHub".into(),
        username: "ada-lovelace".into(),
        url: "https://github.com".into(),
        notes: "rotated".into(),
        password: secret_password("n3w-secret".into()),
        totp_secret: secret_password("JBSWY3DPEHPK3PXP".into()),
        archived: false,
    })?;
    let detail = session.get_password_entry(&updated.entry_id)?;
    if detail.username != "ada-lovelace" || detail.password.expose_secret() != "n3w-secret" {
        return Err(VaultError::Storage("edit did not persist".to_string()));
    }

    let note = session.save_note(&NoteDraft {
        entry_id: None,
        title: "会议".into(),
        content: "买牛奶".into(),
        tags: "生活".into(),
        markdown: false,
    })?;
    let notes = session.list_notes()?;
    if notes.len() != 1 || session.get_note(&note.entry_id)?.content != "买牛奶" {
        return Err(VaultError::Storage(format!("notes mismatch: {notes:?}")));
    }

    let card = session.save_wallet(&WalletDraft {
        entry_id: None,
        kind: WalletKind::Card,
        title: "工资卡".into(),
        holder: "Ada".into(),
        number: secret_password("4111111111111111".into()),
        extra: "Bank".into(),
        expiry: "12/30".into(),
        cvv: secret_password("123".into()),
        notes: String::new(),
    })?;
    let wallet = session.list_wallet()?;
    if wallet.len() != 1 || session.get_wallet(&card.entry_id)?.holder != "Ada" {
        return Err(VaultError::Storage(format!("wallet mismatch: {wallet:?}")));
    }

    let totp = session.save_totp_entry(&TotpDraft {
        entry_id: None,
        source: TotpSource::Standalone,
        title: "GitHub OTP".into(),
        issuer: "GitHub".into(),
        account: "ada".into(),
        secret: secret_password("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".into()),
        period: 30,
        digits: 8,
    })?;
    let totp_detail = session.get_totp_entry(&totp.entry_id, TotpSource::Standalone)?;
    let vector = totp_at(totp_detail.secret.expose_secret(), 59, 30, 8);
    if vector.code != "94287082" {
        return Err(VaultError::Storage(format!("totp vector: {vector:?}")));
    }
    let totp_list = session.list_totp_entries()?;
    if totp_list.len() != 2 {
        return Err(VaultError::Storage(format!(
            "totp list should include standalone + login-bound, got {totp_list:?}"
        )));
    }

    session.set_archived(&note.entry_id, true)?;
    if !session.list_notes()?.is_empty() {
        return Err(VaultError::Storage("archived note still listed".to_string()));
    }
    if session.list_archived()?.len() != 1 {
        return Err(VaultError::Storage("archive list mismatch".to_string()));
    }
    session.set_archived(&note.entry_id, false)?;
    if session.list_notes()?.len() != 1 {
        return Err(VaultError::Storage("unarchive failed".to_string()));
    }

    session.delete_password_entry(&updated.entry_id)?;
    session.delete_note(&note.entry_id)?;
    session.delete_wallet(&card.entry_id)?;
    session.delete_totp_entry(&totp.entry_id, TotpSource::Standalone)?;
    if !session.list_password_entries()?.is_empty()
        || !session.list_notes()?.is_empty()
        || !session.list_wallet()?.is_empty()
    {
        return Err(VaultError::Storage(
            "soft-delete left items visible".to_string(),
        ));
    }
    let trash = session.list_trash()?;
    if trash.len() < 4 {
        return Err(VaultError::Storage(format!("trash mismatch: {trash:?}")));
    }
    session.restore_entry(&updated.entry_id)?;
    if session.list_password_entries()?.len() != 1 {
        return Err(VaultError::Storage("restore did not revive login".to_string()));
    }

    let timeline = session.list_timeline()?;
    if timeline.is_empty() {
        return Err(VaultError::Storage("timeline empty after writes".to_string()));
    }

    let purge = permanent_delete_blocked();
    if !purge.to_string().contains("TIGA") {
        return Err(VaultError::Storage("purge blocked copy mismatch".to_string()));
    }

    let phase3 = phase3_self_test(
        &session,
        path.parent().ok_or_else(|| {
            VaultError::Storage("vault path has no parent directory".to_string())
        })?,
    )?;

    session.lock();
    let locked = session.list_password_entries();
    if !matches!(locked, Err(VaultError::Locked)) {
        return Err(VaultError::Storage(format!(
            "list after lock should fail, got {locked:?}"
        )));
    }

    Ok(format!(
        "crud=ok notes=ok wallet=ok totp=ok archive=ok recycle=ok timeline=ok generator=ok {phase3} lock=ok"
    ))
}

fn phase3_self_test(
    session: &VaultSession,
    directory: &std::path::Path,
) -> Result<String, VaultError> {
    session.save_note(&NoteDraft {
        entry_id: None,
        title: "导出备忘".into(),
        content: "phase3".into(),
        tags: String::new(),
        markdown: false,
    })?;

    let workbench = session.workbench()?;
    if workbench.vault_id != session.info().vault_id {
        return Err(VaultError::Storage("workbench vault_id mismatch".into()));
    }
    if workbench.logins == 0 || workbench.commits == 0 {
        return Err(VaultError::Storage(format!(
            "workbench counts too low: logins={} commits={}",
            workbench.logins, workbench.commits
        )));
    }
    if workbench.requires_upgrade {
        return Err(VaultError::Storage(
            "self-created vault workbench requires upgrade".into(),
        ));
    }

    let backup_path = directory.join("portable-backup.mdbx");
    let backup = session.backup_to(&backup_path)?;
    if backup.vault_id != session.info().vault_id || backup.file_size_bytes == 0 {
        return Err(VaultError::Storage(format!("backup mismatch: {backup:?}")));
    }
    let backup_inspect = inspect_workbench(&backup_path)?;
    if backup_inspect.format_version != session.info().format_version
        || backup_inspect.requires_upgrade
        || backup_inspect.vault_id != backup.vault_id
    {
        return Err(VaultError::Storage(format!(
            "backup inspect mismatch: {backup_inspect:?}"
        )));
    }

    let monica_path = directory.join("monica-export.json");
    let exported = session.export_monica_json(&monica_path)?;
    if exported.logins == 0 || exported.notes == 0 {
        return Err(VaultError::Storage(format!(
            "monica export too small: {}",
            exported.short_status()
        )));
    }
    let before_logins = session.list_password_entries()?.len();
    let imported = session.import_monica_json(&monica_path)?;
    if imported.logins == 0 {
        return Err(VaultError::Storage(format!(
            "monica import empty: {}",
            imported.short_status()
        )));
    }
    if session.list_password_entries()?.len() <= before_logins {
        return Err(VaultError::Storage(
            "monica import did not add logins".into(),
        ));
    }

    let kdbx_path = directory.join("kdbx-export.json");
    let kdbx_export = session.export_kdbx_json(&kdbx_path)?;
    if kdbx_export.logins == 0 {
        return Err(VaultError::Storage("kdbx export empty".into()));
    }
    let before_kdbx = session.list_password_entries()?.len();
    let kdbx_import = session.import_kdbx_json(&kdbx_path)?;
    if kdbx_import.logins == 0 && kdbx_import.warnings.is_empty() {
        return Err(VaultError::Storage(format!(
            "kdbx import empty: {}",
            kdbx_import.short_status()
        )));
    }
    if session.list_password_entries()?.len() < before_kdbx {
        return Err(VaultError::Storage("kdbx import shrank the login list".into()));
    }

    let kdbx_file = directory.join("roundtrip.kdbx");
    let kdbx_password = secret_password("kdbx-file-password".into());
    let kdbx_bin = session.export_kdbx_binary(&kdbx_file, &kdbx_password)?;
    if kdbx_bin.logins == 0 {
        return Err(VaultError::Storage("binary kdbx export empty".into()));
    }
    let before_bin = session.list_password_entries()?.len();
    let kdbx_bin_import = session.import_kdbx_binary(&kdbx_file, &kdbx_password)?;
    if kdbx_bin_import.logins == 0 && kdbx_bin_import.warnings.is_empty() {
        return Err(VaultError::Storage(format!(
            "binary kdbx import empty: {}",
            kdbx_bin_import.short_status()
        )));
    }
    if session.list_password_entries()?.len() < before_bin {
        return Err(VaultError::Storage("binary kdbx import shrank the login list".into()));
    }

    let password_csv = directory.join("passwords.csv");
    let csv_export = session.export_password_csv(&password_csv)?;
    if csv_export.logins == 0 {
        return Err(VaultError::Storage("password csv export empty".into()));
    }
    let before_csv = session.list_password_entries()?.len();
    let csv_import = session.import_csv(&password_csv)?;
    if csv_import.logins == 0 {
        return Err(VaultError::Storage(format!(
            "password csv import empty: {}",
            csv_import.short_status()
        )));
    }
    if session.list_password_entries()?.len() <= before_csv {
        return Err(VaultError::Storage("password csv import did not add logins".into()));
    }

    let combined_csv = directory.join("combined.csv");
    let combined_export = session.export_combined_csv(&combined_csv)?;
    if combined_export.logins == 0 || combined_export.notes == 0 {
        return Err(VaultError::Storage(format!(
            "combined csv too small: {}",
            combined_export.short_status()
        )));
    }
    let before_combined_notes = session.list_notes()?.len();
    let combined_import = session.import_csv(&combined_csv)?;
    if combined_import.notes == 0 {
        return Err(VaultError::Storage(format!(
            "combined csv import empty: {}",
            combined_import.short_status()
        )));
    }
    if session.list_notes()?.len() <= before_combined_notes {
        return Err(VaultError::Storage("combined csv import did not add notes".into()));
    }

    let bundle_path = directory.join("sync-bundle.mdbx-sync");
    let bundle = session.export_sync_bundle(&bundle_path)?;
    if bundle.commits == 0 || bundle.vault_id != session.info().vault_id {
        return Err(VaultError::Storage(format!("sync bundle mismatch: {bundle:?}")));
    }
    let applied = session.apply_sync_bundle(&bundle_path)?;
    if applied.vault_id != session.info().vault_id {
        return Err(VaultError::Storage(format!("sync apply mismatch: {applied:?}")));
    }

    Ok("backup=ok export=ok import=ok kdbx=ok kdbx-bin=ok csv=ok sync-bundle=ok workbench=ok".to_string())
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
        let summary = self_test().expect("phase 2 vault round-trip");
        assert!(summary.contains("format="), "{summary}");
        assert!(summary.contains("unlocked=true"), "{summary}");
        assert!(summary.contains("crud=ok"), "{summary}");
        assert!(summary.contains("notes=ok"), "{summary}");
        assert!(summary.contains("wallet=ok"), "{summary}");
        assert!(summary.contains("totp=ok"), "{summary}");
        assert!(summary.contains("archive=ok"), "{summary}");
        assert!(summary.contains("recycle=ok"), "{summary}");
        assert!(summary.contains("timeline=ok"), "{summary}");
        assert!(summary.contains("generator=ok"), "{summary}");
        assert!(summary.contains("backup=ok"), "{summary}");
        assert!(summary.contains("export=ok"), "{summary}");
        assert!(summary.contains("import=ok"), "{summary}");
        assert!(summary.contains("kdbx=ok"), "{summary}");
        assert!(summary.contains("kdbx-bin=ok"), "{summary}");
        assert!(summary.contains("csv=ok"), "{summary}");
        assert!(summary.contains("sync-bundle=ok"), "{summary}");
        assert!(summary.contains("workbench=ok"), "{summary}");
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
    fn portable_backup_preserves_legacy_mdbx1_without_upgrade() {
        let (directory, path) = temp_vault_path("legacy-backup.mdbx");
        write_legacy_mdbx1_stub(&path);
        let before = inspect::file_fingerprint(&path).expect("fingerprint before");
        let destination = directory.path().join("legacy-copy.mdbx");

        let backup = backup_vault_file(&path, &destination).expect("backup MDBX-1 stub");

        assert_eq!(backup.vault_id, "legacy-avalonia-vault");
        assert_eq!(backup.format_version, "MDBX-1");
        assert_eq!(readonly_format_version(&path), "MDBX-1");
        assert_eq!(readonly_format_version(&destination), "MDBX-1");
        assert_eq!(
            inspect::file_fingerprint(&path).expect("fingerprint after"),
            before,
            "portable backup must not mutate the MDBX-1 source"
        );
        let inspected = inspect_workbench(&destination).expect("inspect backup");
        assert!(inspected.requires_upgrade);
        assert_eq!(inspected.format_version, "MDBX-1");
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
                totp_secret: secret_password(String::new()),
                archived: false,
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
        assert_eq!(session.list_trash().expect("trash").len(), 1);
        session.restore_entry(&saved.entry_id).expect("restore");
        assert_eq!(session.list_password_entries().expect("list").len(), 1);
    }
}
