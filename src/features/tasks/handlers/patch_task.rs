use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::json_field::deserialize_present_field;
use crate::core::state::AppState;
use crate::features::tasks::actions::update_task::UpdateTask;
use crate::features::tasks::entities::task::{
    TaskChanges, validate_estimate_minutes, validate_task_name,
};
use crate::features::tasks::responses::group_tasks_response::GroupTasksResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchTaskRequest {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub estimate_minutes: Option<Option<i64>>,
}

pub async fn patch_task(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(task_id_text): Path<String>,
    Json(patch_request): Json<PatchTaskRequest>,
) -> Result<Json<GroupTasksResponse>, AppError> {
    let task_id = Uuid::parse_str(&task_id_text).map_err(|_| AppError::NotFound)?;
    let task_changes = TaskChanges {
        name: patch_request
            .name
            .as_deref()
            .map(validate_task_name)
            .transpose()?,
        estimate_change: patch_request
            .estimate_minutes
            .map(|estimate_minutes| estimate_minutes.map(validate_estimate_minutes).transpose())
            .transpose()?,
    };
    let group_tasks = UpdateTask::new(&app_state.db_pool)
        .execute(write_session.user_id, task_id, &task_changes)
        .await?;
    Ok(Json(group_tasks.into()))
}
