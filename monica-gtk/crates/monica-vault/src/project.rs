use mdbx_storage::connection::VaultConnection;
use mdbx_storage::repo::{CommitContext, ProjectRepo};

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
