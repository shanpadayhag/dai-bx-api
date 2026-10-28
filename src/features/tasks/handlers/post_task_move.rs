use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::tasks::actions::move_task::{MoveTarget, MoveTask};
use crate::features::tasks::responses::group_tasks_response::MovedTaskResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostTaskMoveRequest {
    pub group_id: String,
    pub parent_id: Option<String>,
    pub position: i64,
}

fn parse_id(id_text: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id_text).map_err(|_| AppError::NotFound)
}

pub async fn post_task_move(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(task_id_text): Path<String>,
    Json(move_request): Json<PostTaskMoveRequest>,
) -> Result<Json<MovedTaskResponse>, AppError> {
    let task_id = parse_id(&task_id_text)?;
    if move_request.position < 0 {
        return Err(AppError::Validation(String::from(
            "a position is 0 or more",
        )));
    }
    let move_target = MoveTarget {
        group_id: parse_id(&move_request.group_id)?,
        parent_id: move_request
            .parent_id
            .as_deref()
            .map(parse_id)
            .transpose()?,
        position: i32::try_from(move_request.position).unwrap_or(i32::MAX),
    };
    let moved_task = MoveTask::new(&app_state.db_pool)
        .execute(write_session.user_id, task_id, move_target)
        .await?;
    Ok(Json(moved_task.into()))
}
