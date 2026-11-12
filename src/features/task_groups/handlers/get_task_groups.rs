use axum::Json;
use axum::extract::State;

use crate::core::error::AppError;
use crate::core::request_time_zone::RequestTimeZone;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::sessions::entities::current_session::CurrentSession;
use crate::features::task_groups::actions::list_task_groups::ListTaskGroups;
use crate::features::task_groups::error::TaskGroupError;
use crate::features::task_groups::responses::task_group_response::TaskGroupResponse;

pub async fn get_task_groups(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    request_time_zone: RequestTimeZone,
) -> Result<Json<Vec<TaskGroupResponse>>, AppError> {
    let mut list_connection = app_state
        .db_pool
        .acquire()
        .await
        .map_err(TaskGroupError::from)?;
    let task_groups = ListTaskGroups::new()
        .execute(
            &mut list_connection,
            current_session.user_id,
            &request_time_zone,
            UtcInstant::now(),
            None,
        )
        .await?;
    Ok(Json(
        task_groups
            .into_iter()
            .map(TaskGroupResponse::from)
            .collect(),
    ))
}
