use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::tasks::actions::reopen_task::ReopenTask;
use crate::features::tasks::responses::group_tasks_response::GroupTasksResponse;
use crate::features::throttling::entities::write_session::WriteSession;

pub async fn post_task_reopen(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(task_id_text): Path<String>,
) -> Result<Json<GroupTasksResponse>, AppError> {
    let task_id = Uuid::parse_str(&task_id_text).map_err(|_| AppError::NotFound)?;
    let group_tasks = ReopenTask::new(&app_state.db_pool)
        .execute(write_session.user_id, task_id)
        .await?;
    Ok(Json(group_tasks.into()))
}
