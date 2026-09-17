//! Authenticated vault session. Owns a [`VaultRuntime`] so GTK can clone the
//! handle onto `gio::spawn_blocking` workers without moving SQLite off a
//! mutex. Locking clears the keyring via [`VaultConnection::clear_session`].

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use mdbx_storage::runtime::VaultRuntime;
use secrecy::SecretString;

use crate::archive::{list_archived, set_archived, ArchivedItem};
use crate::inspect::{create_unlocked_connection, unlock_connection};
use crate::note::{delete_note, get_note, list_notes, save_note, NoteDetail, NoteDraft, NoteSummary};
use crate::password::{
    delete_password_entry, get_password_entry, list_password_entries, save_password_entry,
    PasswordEntryDetail, PasswordEntryDraft, PasswordEntrySummary,
};
use crate::recycle::{list_trash, restore_trash_item, TrashItem};
use crate::timeline::{list_timeline, TimelineItem};
use crate::totp::{
    delete_totp_entry, get_totp_entry, list_totp_entries, save_totp_entry, TotpDetail, TotpDraft,
    TotpSource, TotpSummary,
};
use crate::wallet::{
    delete_wallet, get_wallet, list_wallet, save_wallet, WalletDetail, WalletDraft, WalletSummary,
};
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
