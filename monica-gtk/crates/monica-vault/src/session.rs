//! Authenticated vault session. Owns a [`VaultRuntime`] so GTK can clone the
//! handle onto `gio::spawn_blocking` workers without moving SQLite off a
//! mutex. Locking clears the keyring via [`VaultConnection::clear_session`].

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use mdbx_storage::runtime::VaultRuntime;
use secrecy::SecretString;

use crate::inspect::{create_unlocked_connection, unlock_connection};
use crate::password::{
    delete_password_entry, get_password_entry, list_password_entries, save_password_entry,
    PasswordEntryDetail, PasswordEntryDraft, PasswordEntrySummary,
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
