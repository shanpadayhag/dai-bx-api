use axum::extract::{Path, State};
use axum::http::StatusCode;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::task_groups::actions::delete_task_group::DeleteTaskGroup;
use crate::features::throttling::entities::write_session::WriteSession;

pub async fn delete_task_group(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(task_group_id_text): Path<String>,
) -> Result<StatusCode, AppError> {
    let task_group_id = Uuid::parse_str(&task_group_id_text).map_err(|_| AppError::NotFound)?;
    DeleteTaskGroup::new()
        .execute(&app_state.db_pool, write_session.user_id, task_group_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
