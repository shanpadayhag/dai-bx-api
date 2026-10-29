use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::task_groups::actions::update_task_group::UpdateTaskGroup;
use crate::features::task_groups::entities::task_group::validate_task_group_name;
use crate::features::task_groups::responses::task_group_response::TaskGroupResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
pub struct PatchTaskGroupRequest {
    pub name: String,
}

pub async fn patch_task_group(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(task_group_id_text): Path<String>,
    Json(patch_request): Json<PatchTaskGroupRequest>,
) -> Result<Json<TaskGroupResponse>, AppError> {
    let task_group_id = Uuid::parse_str(&task_group_id_text).map_err(|_| AppError::NotFound)?;
    let task_group_name = validate_task_group_name(&patch_request.name)?;
    let updated_task_group = UpdateTaskGroup::new()
        .execute(
            &app_state.db_pool,
            write_session.user_id,
            task_group_id,
            &task_group_name,
        )
        .await?;
    Ok(Json(updated_task_group.into()))
}
