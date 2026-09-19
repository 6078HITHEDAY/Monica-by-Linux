//! Authenticated vault session. Owns a [`VaultRuntime`] so GTK can clone the
//! handle onto `gio::spawn_blocking` workers without moving SQLite off a
//! mutex. Locking clears the keyring via [`VaultConnection::clear_session`].

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use mdbx_storage::runtime::VaultRuntime;
use secrecy::SecretString;

use crate::archive::{list_archived, set_archived, ArchivedItem};
use crate::backup::{backup_live_connection, VaultBackupInfo};
use crate::csv::{export_combined_csv, export_password_csv, import_csv};
use crate::exchange::{
    export_kdbx_binary, export_kdbx_json, export_monica_json, import_kdbx_binary, import_kdbx_json,
    import_monica_json, TransferSummary,
};
use crate::inspect::{create_unlocked_connection, unlock_connection};
use crate::note::{delete_note, get_note, list_notes, save_note, NoteDetail, NoteDraft, NoteSummary};
use crate::password::{
    delete_password_entry, get_password_entry, list_password_entries, save_password_entry,
    PasswordEntryDetail, PasswordEntryDraft, PasswordEntrySummary,
};
use crate::project::{
    create_project, delete_project, list_projects, move_project_entries, rename_project,
    VaultProject,
};
use crate::recycle::{list_trash, purge_trash_item, restore_trash_item, TrashItem};
use crate::sync::{apply_complete_bundle, export_complete_bundle, SyncApplyInfo, SyncBundleInfo};
use crate::timeline::{list_timeline, TimelineItem};
use crate::totp::{
    delete_totp_entry, get_totp_entry, list_totp_entries, save_totp_entry, TotpDetail, TotpDraft,
    TotpSource, TotpSummary,
};
use crate::wallet::{
    delete_wallet, get_wallet, list_wallet, save_wallet, WalletDetail, WalletDraft, WalletSummary,
};
use crate::workbench::{workbench_from_connection, WorkbenchSnapshot};
use crate::{storage_error, VaultError, VaultInfo};

/// Unlocked vault handle used by the GTK UI after Vault Access succeeds.
#[derive(Clone)]
pub struct VaultSession {
    inner: Arc<SessionInner>,
}

struct SessionInner {
    runtime: VaultRuntime,
    info: VaultInfo,
    live: AtomicBool,
}

impl Drop for SessionInner {
    fn drop(&mut self) {
        clear_runtime(&self.runtime, &self.live);
    }
}

impl VaultSession {
    fn from_parts(runtime: VaultRuntime, info: VaultInfo) -> Self {
        Self {
            inner: Arc::new(SessionInner {
                runtime,
                info,
                live: AtomicBool::new(true),
            }),
        }
    }

    pub fn info(&self) -> &VaultInfo {
        &self.inner.info
    }

    pub fn is_live(&self) -> bool {
        self.inner.live.load(Ordering::Acquire)
    }

    /// Clear keyring/session material. Further entry I/O fails with
    /// [`VaultError::Locked`]. Safe to call more than once.
    pub fn lock(&self) {
        clear_runtime(&self.inner.runtime, &self.inner.live);
    }

    pub fn list_password_entries(&self) -> Result<Vec<PasswordEntrySummary>, VaultError> {
        self.with_read(list_password_entries)
    }

    pub fn get_password_entry(&self, entry_id: &str) -> Result<PasswordEntryDetail, VaultError> {
        self.ensure_live()?;
        let entry_id = entry_id.to_string();
        self.with_read(move |conn| get_password_entry(conn, &entry_id))
    }

    pub fn save_password_entry(
        &self,
        draft: &PasswordEntryDraft,
    ) -> Result<PasswordEntrySummary, VaultError> {
        self.ensure_live()?;
        let draft = draft.clone();
        self.with_write(move |conn| save_password_entry(conn, &draft))
    }

    pub fn delete_password_entry(&self, entry_id: &str) -> Result<(), VaultError> {
        self.ensure_live()?;
        let entry_id = entry_id.to_string();
        self.with_write(move |conn| delete_password_entry(conn, &entry_id))
    }

    pub fn list_projects(&self) -> Result<Vec<VaultProject>, VaultError> {
        self.with_read(list_projects)
    }

    pub fn create_project(&self, title: &str) -> Result<VaultProject, VaultError> {
        self.ensure_live()?;
        let title = title.to_string();
        self.with_write(move |conn| create_project(conn, &title))
    }

    pub fn rename_project(&self, project_id: &str, title: &str) -> Result<VaultProject, VaultError> {
        self.ensure_live()?;
        let project_id = project_id.to_string();
        let title = title.to_string();
        self.with_write(move |conn| rename_project(conn, &project_id, &title))
    }

    pub fn delete_project(&self, project_id: &str) -> Result<(), VaultError> {
        self.ensure_live()?;
        let project_id = project_id.to_string();
        self.with_write(move |conn| delete_project(conn, &project_id))
    }

    pub fn move_project_entries(
        &self,
        from_id: &str,
        to_id: &str,
    ) -> Result<usize, VaultError> {
        self.ensure_live()?;
        let from_id = from_id.to_string();
        let to_id = to_id.to_string();
        self.with_write(move |conn| move_project_entries(conn, &from_id, &to_id))
    }

    /// Soft-delete any entry type (login, note, card, totp, …).
    pub fn delete_entry(&self, entry_id: &str) -> Result<(), VaultError> {
        self.delete_password_entry(entry_id)
    }

    pub fn list_notes(&self) -> Result<Vec<NoteSummary>, VaultError> {
        self.with_read(list_notes)
    }

    pub fn get_note(&self, entry_id: &str) -> Result<NoteDetail, VaultError> {
        self.ensure_live()?;
        let entry_id = entry_id.to_string();
        self.with_read(move |conn| get_note(conn, &entry_id))
    }

    pub fn save_note(&self, draft: &NoteDraft) -> Result<NoteSummary, VaultError> {
        self.ensure_live()?;
        let draft = draft.clone();
        self.with_write(move |conn| save_note(conn, &draft))
    }

    pub fn delete_note(&self, entry_id: &str) -> Result<(), VaultError> {
        self.ensure_live()?;
        let entry_id = entry_id.to_string();
        self.with_write(move |conn| delete_note(conn, &entry_id))
    }

    pub fn list_wallet(&self) -> Result<Vec<WalletSummary>, VaultError> {
        self.with_read(list_wallet)
    }

    pub fn get_wallet(&self, entry_id: &str) -> Result<WalletDetail, VaultError> {
        self.ensure_live()?;
        let entry_id = entry_id.to_string();
        self.with_read(move |conn| get_wallet(conn, &entry_id))
    }

    pub fn save_wallet(&self, draft: &WalletDraft) -> Result<WalletSummary, VaultError> {
        self.ensure_live()?;
        let draft = draft.clone();
        self.with_write(move |conn| save_wallet(conn, &draft))
    }

    pub fn delete_wallet(&self, entry_id: &str) -> Result<(), VaultError> {
        self.ensure_live()?;
        let entry_id = entry_id.to_string();
        self.with_write(move |conn| delete_wallet(conn, &entry_id))
    }

    pub fn list_totp_entries(&self) -> Result<Vec<TotpSummary>, VaultError> {
        self.with_read(list_totp_entries)
    }

    pub fn get_totp_entry(
        &self,
        entry_id: &str,
        source: TotpSource,
    ) -> Result<TotpDetail, VaultError> {
        self.ensure_live()?;
        let entry_id = entry_id.to_string();
        self.with_read(move |conn| get_totp_entry(conn, &entry_id, source))
    }

    pub fn save_totp_entry(&self, draft: &TotpDraft) -> Result<TotpSummary, VaultError> {
        self.ensure_live()?;
        let draft = draft.clone();
        self.with_write(move |conn| save_totp_entry(conn, &draft))
    }

    pub fn delete_totp_entry(&self, entry_id: &str, source: TotpSource) -> Result<(), VaultError> {
        self.ensure_live()?;
        let entry_id = entry_id.to_string();
        self.with_write(move |conn| delete_totp_entry(conn, &entry_id, source))
    }

    pub fn list_trash(&self) -> Result<Vec<TrashItem>, VaultError> {
        self.with_read(list_trash)
    }

    pub fn restore_entry(&self, entry_id: &str) -> Result<TrashItem, VaultError> {
        self.ensure_live()?;
        let entry_id = entry_id.to_string();
        self.with_write(move |conn| restore_trash_item(conn, &entry_id))
    }

    pub fn purge_entry(&self, entry_id: &str) -> Result<(), VaultError> {
        self.ensure_live()?;
        let entry_id = entry_id.to_string();
        self.with_write(move |conn| purge_trash_item(conn, &entry_id))
    }

    pub fn list_archived(&self) -> Result<Vec<ArchivedItem>, VaultError> {
        self.with_read(list_archived)
    }

    pub fn set_archived(&self, entry_id: &str, archived: bool) -> Result<ArchivedItem, VaultError> {
        self.ensure_live()?;
        let entry_id = entry_id.to_string();
        self.with_write(move |conn| set_archived(conn, &entry_id, archived))
    }

    pub fn list_timeline(&self) -> Result<Vec<TimelineItem>, VaultError> {
        self.with_read(list_timeline)
    }

    /// Live portable copy of the open vault (WAL-consistent). Destination must be new.
    pub fn backup_to(&self, destination: &Path) -> Result<VaultBackupInfo, VaultError> {
        self.ensure_live()?;
        let destination = destination.to_path_buf();
        self.with_read(move |conn| backup_live_connection(conn, &destination))
    }

    pub fn workbench(&self) -> Result<WorkbenchSnapshot, VaultError> {
        self.ensure_live()?;
        let path = self.inner.info.path.clone();
        self.with_read(move |conn| workbench_from_connection(conn, &path, true))
    }

    pub fn export_monica_json(&self, destination: &Path) -> Result<TransferSummary, VaultError> {
        self.ensure_live()?;
        let destination = destination.to_path_buf();
        let vault_id = self.inner.info.vault_id.clone();
        self.with_read(move |conn| export_monica_json(conn, &vault_id, &destination))
    }

    pub fn import_monica_json(&self, source: &Path) -> Result<TransferSummary, VaultError> {
        self.ensure_live()?;
        let source = source.to_path_buf();
        self.with_write(move |conn| import_monica_json(conn, &source))
    }

    pub fn export_kdbx_json(&self, destination: &Path) -> Result<TransferSummary, VaultError> {
        self.ensure_live()?;
        let destination = destination.to_path_buf();
        self.with_read(move |conn| export_kdbx_json(conn, &destination))
    }

    pub fn import_kdbx_json(&self, source: &Path) -> Result<TransferSummary, VaultError> {
        self.ensure_live()?;
        let source = source.to_path_buf();
        self.with_write(move |conn| import_kdbx_json(conn, &source))
    }

    pub fn export_kdbx_binary(
        &self,
        destination: &Path,
        password: &SecretString,
    ) -> Result<TransferSummary, VaultError> {
        self.ensure_live()?;
        let destination = destination.to_path_buf();
        let password = password.clone();
        self.with_read(move |conn| export_kdbx_binary(conn, &destination, &password))
    }

    pub fn import_kdbx_binary(
        &self,
        source: &Path,
        password: &SecretString,
    ) -> Result<TransferSummary, VaultError> {
        self.ensure_live()?;
        let source = source.to_path_buf();
        let password = password.clone();
        self.with_write(move |conn| import_kdbx_binary(conn, &source, &password))
    }

    pub fn export_password_csv(&self, destination: &Path) -> Result<TransferSummary, VaultError> {
        self.ensure_live()?;
        let destination = destination.to_path_buf();
        self.with_read(move |conn| export_password_csv(conn, &destination))
    }

    pub fn export_combined_csv(&self, destination: &Path) -> Result<TransferSummary, VaultError> {
        self.ensure_live()?;
        let destination = destination.to_path_buf();
        self.with_read(move |conn| export_combined_csv(conn, &destination))
    }

    pub fn import_csv(&self, source: &Path) -> Result<TransferSummary, VaultError> {
        self.ensure_live()?;
        let source = source.to_path_buf();
        self.with_write(move |conn| import_csv(conn, &source))
    }

    pub fn export_sync_bundle(&self, destination: &Path) -> Result<SyncBundleInfo, VaultError> {
        self.ensure_live()?;
        let destination = destination.to_path_buf();
        self.with_read(move |conn| export_complete_bundle(conn, &destination))
    }

    pub fn apply_sync_bundle(&self, source: &Path) -> Result<SyncApplyInfo, VaultError> {
        self.ensure_live()?;
        let source = source.to_path_buf();
        self.with_write_mut(move |conn| apply_complete_bundle(conn, &source))
    }

    fn ensure_live(&self) -> Result<(), VaultError> {
        if self.is_live() {
            Ok(())
        } else {
            Err(VaultError::Locked)
        }
    }

    fn with_read<T>(
        &self,
        work: impl FnOnce(&mdbx_storage::connection::VaultConnection) -> Result<T, VaultError>,
    ) -> Result<T, VaultError> {
        self.ensure_live()?;
        let mut result = None;
        self.inner
            .runtime
            .with_read(|conn| {
                result = Some(work(conn));
                Ok(())
            })
            .map_err(storage_error)?;
        result.expect("read work ran")
    }

    fn with_write<T>(
        &self,
        work: impl FnOnce(&mdbx_storage::connection::VaultConnection) -> Result<T, VaultError>,
    ) -> Result<T, VaultError> {
        self.ensure_live()?;
        let mut result = None;
        self.inner
            .runtime
            .with_write(|conn| {
                result = Some(work(conn));
                Ok(())
            })
            .map_err(storage_error)?;
        result.expect("write work ran")
    }

    fn with_write_mut<T>(
        &self,
        work: impl FnOnce(&mut mdbx_storage::connection::VaultConnection) -> Result<T, VaultError>,
    ) -> Result<T, VaultError> {
        self.ensure_live()?;
        let mut result = None;
        self.inner
            .runtime
            .with_write(|conn| {
                result = Some(work(conn));
                Ok(())
            })
            .map_err(storage_error)?;
        result.expect("write work ran")
    }
}

/// Create a vault file, configure password unlock, and return a live session.
pub fn create_session(path: &Path, password: &SecretString) -> Result<VaultSession, VaultError> {
    let (connection, info) = create_unlocked_connection(path, password)?;
    Ok(VaultSession::from_parts(
        VaultRuntime::from_connection(connection),
        info,
    ))
}

/// Open an existing current-format vault and attach an authenticated session.
pub fn unlock_session(path: &Path, password: &SecretString) -> Result<VaultSession, VaultError> {
    let (connection, info) = unlock_connection(path, password)?;
    Ok(VaultSession::from_parts(
        VaultRuntime::from_connection(connection),
        info,
    ))
}

fn clear_runtime(runtime: &VaultRuntime, live: &AtomicBool) {
    if !live.swap(false, Ordering::AcqRel) {
        return;
    }
    let _ = runtime.with_write(|connection| {
        connection.clear_session();
        Ok(())
    });
}
