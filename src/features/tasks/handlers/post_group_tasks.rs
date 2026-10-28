use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::tasks::actions::add_task::AddTask;
use crate::features::tasks::entities::task::validate_task_name;
use crate::features::tasks::responses::group_tasks_response::GroupTasksResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostGroupTasksRequest {
    pub name: String,
    pub parent_id: Option<String>,
}

pub async fn post_group_tasks(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(task_group_id_text): Path<String>,
    Json(post_request): Json<PostGroupTasksRequest>,
) -> Result<Json<GroupTasksResponse>, AppError> {
    let task_group_id = Uuid::parse_str(&task_group_id_text).map_err(|_| AppError::NotFound)?;
    let task_name = validate_task_name(&post_request.name)?;
    let parent_task_id = post_request
        .parent_id
        .as_deref()
        .map(|parent_id_text| Uuid::parse_str(parent_id_text).map_err(|_| AppError::NotFound))
        .transpose()?;
    let group_tasks = AddTask::new(&app_state.db_pool)
        .execute(
            write_session.user_id,
            task_group_id,
            &task_name,
            parent_task_id,
        )
        .await?;
    Ok(Json(group_tasks.into()))
}
