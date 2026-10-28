use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::json_field::deserialize_present_field;
use crate::core::state::AppState;
use crate::features::task_groups::actions::update_task_group::UpdateTaskGroup;
use crate::features::task_groups::entities::task_group::{
    TaskGroupChanges, validate_task_group_name,
};
use crate::features::task_groups::responses::task_group_response::TaskGroupResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchTaskGroupRequest {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub category_id: Option<Option<String>>,
}

pub async fn patch_task_group(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(task_group_id_text): Path<String>,
    Json(patch_request): Json<PatchTaskGroupRequest>,
) -> Result<Json<TaskGroupResponse>, AppError> {
    let task_group_id = Uuid::parse_str(&task_group_id_text).map_err(|_| AppError::NotFound)?;
    let task_group_changes = TaskGroupChanges {
        name: patch_request
            .name
            .as_deref()
            .map(validate_task_group_name)
            .transpose()?,
        category_change: patch_request
            .category_id
            .map(|category_id_text| {
                category_id_text
                    .as_deref()
                    .map(|category_id_text| {
                        Uuid::parse_str(category_id_text).map_err(|_| AppError::NotFound)
                    })
                    .transpose()
            })
            .transpose()?,
    };
    let updated_task_group = UpdateTaskGroup::new()
        .execute(
            &app_state.db_pool,
            write_session.user_id,
            task_group_id,
            &task_group_changes,
        )
        .await?;
    Ok(Json(updated_task_group.into()))
}
