use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::request_time_zone::RequestTimeZone;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::task_groups::actions::update_task_group::UpdateTaskGroup;
use crate::features::task_groups::handlers::post_task_groups::TaskGroupRequest;
use crate::features::task_groups::responses::task_group_response::TaskGroupResponse;
use crate::features::throttling::entities::write_session::WriteSession;

pub async fn patch_task_group(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    request_time_zone: RequestTimeZone,
    Path(task_group_id_text): Path<String>,
    Json(task_group_request): Json<TaskGroupRequest>,
) -> Result<Json<TaskGroupResponse>, AppError> {
    let task_group_id = Uuid::parse_str(&task_group_id_text).map_err(|_| AppError::NotFound)?;
    let (task_group_name, group_rule_changes) = task_group_request.into_name_and_changes()?;
    let updated_task_group = UpdateTaskGroup::new(&app_state.db_pool)
        .execute(
            write_session.user_id,
            task_group_id,
            &task_group_name,
            &group_rule_changes,
            &request_time_zone,
            UtcInstant::now(),
        )
        .await?;
    Ok(Json(updated_task_group.into()))
}
