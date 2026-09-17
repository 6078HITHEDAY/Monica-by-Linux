//! Read-only vault workbench: format, schema, counts, migration plan.
//!
//! Never opens the file for write and never runs an in-place MDBX-1 upgrade.

use std::path::{Path, PathBuf};

use rusqlite::OptionalExtension;

use crate::inspect::{inspect_existing, inspect_vault};
use crate::{storage_error, VaultError, VaultInfo};
use mdbx_storage::connection::VaultConnection;
use mdbx_storage::migration;

/// Snapshot used by the MDBX 工作台 page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkbenchSnapshot {
    pub path: PathBuf,
    pub vault_id: String,
    pub format_version: String,
    pub schema_version: i64,
    pub tiga_mode: String,
    pub unlocked: bool,
    pub requires_upgrade: bool,
    pub target_format_version: String,
    pub target_schema_version: i64,
    pub min_reader_version: String,
    pub min_writer_version: String,
    pub unknown_critical_extensions: bool,
    pub file_size_bytes: u64,
    pub logins: u64,
    pub notes: u64,
    pub cards: u64,
    pub totp: u64,
    pub documents: u64,
    pub other_entries: u64,
    pub deleted_entries: u64,
    pub projects: u64,
    pub commits: u64,
}

impl WorkbenchSnapshot {
    pub fn upgrade_hint(&self) -> Option<String> {
        if !self.requires_upgrade {
            return None;
        }
        Some(format!(
            "当前为 {} schema {}，原地打开会升到 {} schema {}。请先备份或复制，不要在此客户端就地升级。",
            self.format_version,
            self.schema_version,
            self.target_format_version,
            self.target_schema_version
        ))
    }
}

/// Inspect a vault file without unlocking. Counts come from plaintext catalog
/// columns (`entry_type`, `deleted`) and do not decrypt payloads.
///
/// When a live session has the file open, prefer [`workbench_from_connection`]
/// so WAL pages are included.
pub fn inspect_workbench(path: &Path) -> Result<WorkbenchSnapshot, VaultError> {
    let info = inspect_vault(path)?;
    workbench_from_info(info)
}

pub(crate) fn workbench_from_connection(
    conn: &VaultConnection,
    path: &Path,
    unlocked: bool,
) -> Result<WorkbenchSnapshot, VaultError> {
    let inner = conn.inner();
    let migration = migration::inspect_migration(inner).map_err(storage_error)?;
    let vault_id: String = inner
        .query_row("SELECT vault_id FROM vault_meta LIMIT 1", [], |row| row.get(0))
        .map_err(storage_error)?;
    let tiga_mode = inner
        .query_row(
            "SELECT default_tiga_mode FROM vault_meta LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_else(|_| "unknown".to_string());
    let file_size_bytes = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    let counts = count_catalog(inner).unwrap_or_default();
    Ok(WorkbenchSnapshot {
        path: path.to_path_buf(),
        vault_id,
        format_version: migration
            .format_version
            .unwrap_or_else(|| "unknown".to_string()),
        schema_version: i64::from(migration.schema_version.unwrap_or(0)),
        tiga_mode,
        unlocked,
        requires_upgrade: migration.requires_upgrade,
        target_format_version: migration.target_format_version,
        target_schema_version: i64::from(migration.target_schema_version),
        min_reader_version: migration.min_reader_version.unwrap_or_else(|| "—".into()),
        min_writer_version: migration.min_writer_version.unwrap_or_else(|| "—".into()),
        unknown_critical_extensions: migration.unknown_critical_extensions,
        file_size_bytes,
        logins: counts.logins,
        notes: counts.notes,
        cards: counts.cards,
        totp: counts.totp,
        documents: counts.documents,
        other_entries: counts.other_entries,
        deleted_entries: counts.deleted_entries,
        projects: counts.projects,
        commits: counts.commits,
    })
}

pub(crate) fn workbench_from_info(info: VaultInfo) -> Result<WorkbenchSnapshot, VaultError> {
    let (connection, migration) = inspect_existing(&info.path)?;
    let file_size_bytes = std::fs::metadata(&info.path)
        .map(|meta| meta.len())
        .unwrap_or(0);
    let counts = count_catalog(&connection).unwrap_or_default();
    Ok(WorkbenchSnapshot {
        path: info.path,
        vault_id: info.vault_id,
        format_version: info.format_version,
        schema_version: info.schema_version,
        tiga_mode: info.tiga_mode,
        unlocked: info.unlocked,
        requires_upgrade: info.requires_upgrade,
        target_format_version: migration.target_format_version,
        target_schema_version: i64::from(migration.target_schema_version),
        min_reader_version: migration.min_reader_version.unwrap_or_else(|| "—".into()),
        min_writer_version: migration.min_writer_version.unwrap_or_else(|| "—".into()),
        unknown_critical_extensions: migration.unknown_critical_extensions,
        file_size_bytes,
        logins: counts.logins,
        notes: counts.notes,
        cards: counts.cards,
        totp: counts.totp,
        documents: counts.documents,
        other_entries: counts.other_entries,
        deleted_entries: counts.deleted_entries,
        projects: counts.projects,
        commits: counts.commits,
    })
}

#[derive(Default)]
struct CatalogCounts {
    logins: u64,
    notes: u64,
    cards: u64,
    totp: u64,
    documents: u64,
    other_entries: u64,
    deleted_entries: u64,
    projects: u64,
    commits: u64,
}

fn table_exists(connection: &rusqlite::Connection, name: &str) -> bool {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |_| Ok(()),
        )
        .optional()
        .ok()
        .flatten()
        .is_some()
}

fn count_catalog(connection: &rusqlite::Connection) -> Result<CatalogCounts, VaultError> {
    let mut counts = CatalogCounts::default();
    if table_exists(connection, "entries") {
        let mut statement = connection
            .prepare(
                "SELECT entry_type, COALESCE(deleted, 0), COUNT(*)
                 FROM entries
                 GROUP BY entry_type, COALESCE(deleted, 0)",
            )
            .map_err(storage_error)?;
        let mut rows = statement.query([]).map_err(storage_error)?;
        while let Some(row) = rows.next().map_err(storage_error)? {
            let kind: String = row.get(0).map_err(storage_error)?;
            let deleted: i64 = row.get(1).map_err(storage_error)?;
            let count: i64 = row.get(2).map_err(storage_error)?;
            let count = u64::try_from(count).unwrap_or(0);
            if deleted != 0 {
                counts.deleted_entries += count;
                continue;
            }
            match kind.as_str() {
                "login" => counts.logins += count,
                "note" => counts.notes += count,
                "card" => counts.cards += count,
                "totp" => counts.totp += count,
                "document-ref" => counts.documents += count,
                _ => counts.other_entries += count,
            }
        }
    }
    if table_exists(connection, "projects") {
        counts.projects = connection
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get::<_, i64>(0))
            .map_err(storage_error)
            .map(|value| u64::try_from(value).unwrap_or(0))?;
    }
    if table_exists(connection, "commits") {
        counts.commits = connection
            .query_row("SELECT COUNT(*) FROM commits", [], |row| row.get::<_, i64>(0))
            .map_err(storage_error)
            .map(|value| u64::try_from(value).unwrap_or(0))?;
    }
    Ok(counts)
}
