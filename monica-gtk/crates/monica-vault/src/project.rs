use mdbx_storage::connection::VaultConnection;
use mdbx_storage::repo::{CommitContext, ProjectRepo};

use crate::{storage_error, VaultError};

const DEFAULT_PROJECT_TITLE: &str = "Monica";

pub(crate) fn ensure_default_project(
    conn: &VaultConnection,
    ctx: &CommitContext,
) -> Result<String, VaultError> {
    let projects = ProjectRepo::list_all(conn).map_err(storage_error)?;
    if let Some(existing) = projects.into_iter().next() {
        return Ok(existing.project_id);
    }
    let created =
        ProjectRepo::create(conn, ctx, DEFAULT_PROJECT_TITLE, None, None).map_err(storage_error)?;
    Ok(created.project_id)
}
