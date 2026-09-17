//! Vault activity timeline from upstream `CommitHistoryRepo`.

use mdbx_storage::connection::VaultConnection;
use mdbx_storage::repo::CommitHistoryRepo;

use crate::{storage_error, VaultError};

const DEFAULT_PAGE: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineItem {
    pub commit_id: String,
    pub created_at: String,
    pub kind: String,
    pub scope: String,
    pub summary: String,
    pub message: Option<String>,
}

pub(crate) fn list_timeline(conn: &VaultConnection) -> Result<Vec<TimelineItem>, VaultError> {
    let page = CommitHistoryRepo::list(conn, DEFAULT_PAGE, None).map_err(storage_error)?;
    Ok(page
        .items
        .into_iter()
        .map(|item| {
            let summary = if item.changes.is_empty() {
                item.operation_kind
                    .clone()
                    .unwrap_or_else(|| item.commit_kind.clone())
            } else {
                item.changes
                    .iter()
                    .take(3)
                    .map(|change| format!("{} {}", change.object_type, change.action))
                    .collect::<Vec<_>>()
                    .join(" · ")
            };
            TimelineItem {
                commit_id: item.commit_id,
                created_at: item.created_at,
                kind: item
                    .operation_kind
                    .unwrap_or(item.commit_kind),
                scope: item.change_scope,
                summary,
                message: item.message.filter(|text| !text.trim().is_empty()),
            }
        })
        .collect())
}
