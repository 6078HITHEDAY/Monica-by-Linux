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

/// Live vs recycle-bin occupancy for one folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderSummary {
    pub project_id: String,
    pub title: String,
    pub live: usize,
    pub trash: usize,
}

impl FolderSummary {
    pub fn is_empty(&self) -> bool {
        self.live == 0 && self.trash == 0
    }
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
    sort_projects(&mut listed);
    Ok(listed)
}

pub(crate) fn list_folder_summaries(
    conn: &VaultConnection,
) -> Result<Vec<FolderSummary>, VaultError> {
    let projects = list_projects(conn)?;
    let mut summaries = Vec::with_capacity(projects.len());
    for project in projects {
        summaries.push(folder_summary(conn, &project)?);
    }
    Ok(summaries)
}

pub(crate) fn create_project(
    conn: &VaultConnection,
    title: &str,
) -> Result<VaultProject, VaultError> {
    let title = require_title(title)?;
    let ctx = CommitContext::new(DEVICE_ID.to_string());
    let created = ProjectRepo::create(conn, &ctx, &title, None, None).map_err(storage_error)?;
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
    let title = require_title(title)?;
    let ctx = CommitContext::new(DEVICE_ID.to_string());
    let mut project = ProjectRepo::get_by_id(conn, project_id)
        .map_err(storage_error)?
        .ok_or_else(|| VaultError::EntryNotFound(project_id.to_string()))?;
    if project.deleted {
        return Err(VaultError::Storage("文件夹已删除".to_string()));
    }
    project.title_ct = title.as_bytes().to_vec();
    let updated = ProjectRepo::update(conn, &ctx, &project).map_err(storage_error)?;
    Ok(VaultProject {
        project_id: updated.project_id,
        title: project_title_from_bytes(&updated.title_ct),
    })
}

/// Move live entries to another folder. Recycle-bin items stay put:
/// upstream [`EntryRepo::move_to_project`] refuses deleted entries.
/// Folders themselves cannot be relocated (only unused `group_id`).
pub(crate) fn migrate_project_entries(
    conn: &VaultConnection,
    from_id: &str,
    to_id: &str,
) -> Result<u32, VaultError> {
    if from_id == to_id {
        return Err(VaultError::Storage("请选择不同的文件夹".to_string()));
    }
    ensure_active_folder(conn, from_id)?;
    ensure_active_folder(conn, to_id)?;
    let ctx = CommitContext::new(DEVICE_ID.to_string());
    let live = EntryRepo::list_by_project(conn, from_id).map_err(storage_error)?;
    let mut moved = 0u32;
    for entry in live {
        EntryRepo::move_to_project(conn, &ctx, &entry.entry_id, to_id).map_err(storage_error)?;
        moved += 1;
    }
    Ok(moved)
}

/// Soft-delete a folder only when it has no live or recycle-bin entries.
/// [`ProjectRepo::soft_delete`] does not remove or rehome entries.
pub(crate) fn delete_project(conn: &VaultConnection, project_id: &str) -> Result<(), VaultError> {
    let project = ensure_active_folder(conn, project_id)?;
    let occupancy = folder_summary(conn, &project)?;
    if !occupancy.is_empty() {
        return Err(VaultError::Storage(format!(
            "文件夹还有 {} 条在用、{} 条在回收站。请先迁出在用条目；回收站条目无法迁移。",
            occupancy.live, occupancy.trash
        )));
    }
    let ctx = CommitContext::new(DEVICE_ID.to_string());
    ProjectRepo::soft_delete(conn, &ctx, project_id).map_err(storage_error)
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

fn require_title(title: &str) -> Result<String, VaultError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(VaultError::Storage("请填写名称".to_string()));
    }
    Ok(title.to_string())
}

fn ensure_active_folder(
    conn: &VaultConnection,
    project_id: &str,
) -> Result<VaultProject, VaultError> {
    let project = ProjectRepo::get_by_id(conn, project_id)
        .map_err(storage_error)?
        .ok_or_else(|| VaultError::EntryNotFound(project_id.to_string()))?;
    if project.deleted {
        return Err(VaultError::Storage("文件夹已删除".to_string()));
    }
    Ok(VaultProject {
        project_id: project.project_id,
        title: project_title_from_bytes(&project.title_ct),
    })
}

fn folder_summary(
    conn: &VaultConnection,
    project: &VaultProject,
) -> Result<FolderSummary, VaultError> {
    let live = EntryRepo::list_by_project(conn, &project.project_id).map_err(storage_error)?;
    let trash =
        EntryRepo::list_deleted_by_project(conn, &project.project_id).map_err(storage_error)?;
    Ok(FolderSummary {
        project_id: project.project_id.clone(),
        title: project.title.clone(),
        live: live.len(),
        trash: trash.len(),
    })
}

fn sort_projects(listed: &mut [VaultProject]) {
    listed.sort_by(|left, right| {
        let left_default = left.title == DEFAULT_PROJECT_TITLE;
        let right_default = right.title == DEFAULT_PROJECT_TITLE;
        right_default
            .cmp(&left_default)
            .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
            .then_with(|| left.project_id.cmp(&right.project_id))
    });
}
