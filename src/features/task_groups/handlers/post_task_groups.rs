use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::task_groups::actions::create_task_group::CreateTaskGroup;
use crate::features::task_groups::entities::task_group::validate_task_group_name;
use crate::features::task_groups::responses::task_group_response::TaskGroupResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostTaskGroupsRequest {
    pub name: String,
    pub category_id: Option<String>,
}

pub async fn post_task_groups(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Json(post_request): Json<PostTaskGroupsRequest>,
) -> Result<(StatusCode, Json<TaskGroupResponse>), AppError> {
    let task_group_name = validate_task_group_name(&post_request.name)?;
    let category_id = post_request
        .category_id
        .as_deref()
        .map(|category_id_text| Uuid::parse_str(category_id_text).map_err(|_| AppError::NotFound))
        .transpose()?;
    let created_task_group = CreateTaskGroup::new()
        .execute(
            &app_state.db_pool,
            write_session.user_id,
            &task_group_name,
            category_id,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(created_task_group.into())))
}
