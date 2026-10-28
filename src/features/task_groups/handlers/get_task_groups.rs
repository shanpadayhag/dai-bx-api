use axum::Json;
use axum::extract::State;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::sessions::entities::current_session::CurrentSession;
use crate::features::task_groups::actions::list_task_groups::ListTaskGroups;
use crate::features::task_groups::responses::task_group_response::TaskGroupResponse;

pub async fn get_task_groups(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
) -> Result<Json<Vec<TaskGroupResponse>>, AppError> {
    let task_groups = ListTaskGroups::new()
        .execute(&app_state.db_pool, current_session.user_id)
        .await?;
    Ok(Json(
        task_groups
            .into_iter()
            .map(TaskGroupResponse::from)
            .collect(),
    ))
}
