use mdbx_storage::connection::VaultConnection;
use mdbx_storage::repo::{CommitContext, EntryRepo, ProjectRepo};

use crate::{storage_error, VaultError, DEVICE_ID};

const DEFAULT_PROJECT_TITLE: &str = "Monica";

/// Password-library folder backed by upstream [`ProjectRepo`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultProject {
    pub project_id: String,
    pub title: String,
}

pub(crate) fn project_title_from_bytes(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_string()
}

pub(crate) fn list_projects(conn: &VaultConnection) -> Result<Vec<VaultProject>, VaultError> {
    let projects = ProjectRepo::list_all(conn).map_err(storage_error)?;
    let mut listed: Vec<VaultProject> = projects
        .into_iter()
        .map(|project| VaultProject {
            project_id: project.project_id,
            title: project_title_from_bytes(&project.title_ct),
        })
        .collect();
    listed.sort_by(|left, right| {
        let left_default = left.title == DEFAULT_PROJECT_TITLE;
        let right_default = right.title == DEFAULT_PROJECT_TITLE;
        right_default
            .cmp(&left_default)
            .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
            .then_with(|| left.project_id.cmp(&right.project_id))
    });
    Ok(listed)
}

pub(crate) fn create_project(
    conn: &VaultConnection,
    title: &str,
) -> Result<VaultProject, VaultError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(VaultError::Storage("请填写名称".to_string()));
    }
    let ctx = CommitContext::new(DEVICE_ID.to_string());
    let created =
        ProjectRepo::create(conn, &ctx, title, None, None).map_err(storage_error)?;
    Ok(VaultProject {
        project_id: created.project_id,
        title: project_title_from_bytes(&created.title_ct),
    })
}

pub(crate) fn rename_project(
    conn: &VaultConnection,
    project_id: &str,
    title: &str,
) -> Result<VaultProject, VaultError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(VaultError::Storage("请填写名称".to_string()));
    }
    let ctx = CommitContext::new(DEVICE_ID.to_string());
    let mut project = ProjectRepo::get_by_id(conn, project_id)
        .map_err(storage_error)?
        .ok_or_else(|| VaultError::EntryNotFound(project_id.to_string()))?;
    if project.deleted {
        return Err(VaultError::EntryNotFound(project_id.to_string()));
    }
    project.title_ct = title.as_bytes().to_vec();
    let updated = ProjectRepo::update(conn, &ctx, &project).map_err(storage_error)?;
    Ok(VaultProject {
        project_id: updated.project_id,
        title: project_title_from_bytes(&updated.title_ct),
    })
}

/// Soft-delete a folder only when no live or recycle-bin entries still point at it.
pub(crate) fn delete_project(conn: &VaultConnection, project_id: &str) -> Result<(), VaultError> {
    let live = EntryRepo::list_by_project(conn, project_id).map_err(storage_error)?;
    let trashed = EntryRepo::list_deleted_by_project(conn, project_id).map_err(storage_error)?;
    if !live.is_empty() || !trashed.is_empty() {
        return Err(VaultError::Storage(
            "文件夹非空，请先移出条目".to_string(),
        ));
    }
    let ctx = CommitContext::new(DEVICE_ID.to_string());
    ProjectRepo::soft_delete(conn, &ctx, project_id).map_err(|error| match error {
        mdbx_storage::error::StorageError::NotFound(_) => {
            VaultError::EntryNotFound(project_id.to_string())
        }
        other => storage_error(other),
    })
}

/// Move live entries from one folder to another. Deleted entries stay put.
pub(crate) fn move_project_entries(
    conn: &VaultConnection,
    from_id: &str,
    to_id: &str,
) -> Result<usize, VaultError> {
    if from_id == to_id {
        return Err(VaultError::Storage("请选择其他文件夹".to_string()));
    }
    let ctx = CommitContext::new(DEVICE_ID.to_string());
    let entries = EntryRepo::list_by_project(conn, from_id).map_err(storage_error)?;
    for entry in &entries {
        EntryRepo::move_to_project(conn, &ctx, &entry.entry_id, to_id).map_err(storage_error)?;
    }
    Ok(entries.len())
}

pub(crate) fn ensure_default_project(
    conn: &VaultConnection,
    ctx: &CommitContext,
) -> Result<String, VaultError> {
    let listed = list_projects(conn)?;
    if let Some(existing) = listed
        .iter()
        .find(|project| project.title == DEFAULT_PROJECT_TITLE)
    {
        return Ok(existing.project_id.clone());
    }
    let created =
        ProjectRepo::create(conn, ctx, DEFAULT_PROJECT_TITLE, None, None).map_err(storage_error)?;
    Ok(created.project_id)
}
