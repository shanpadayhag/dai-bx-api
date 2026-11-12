use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::request_time_zone::RequestTimeZone;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::sessions::entities::current_session::CurrentSession;
use crate::features::tasks::actions::find_group_tasks::FindGroupTasks;
use crate::features::tasks::responses::group_tasks_response::GroupTasksResponse;

pub async fn get_group_tasks(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    request_time_zone: RequestTimeZone,
    Path(task_group_id_text): Path<String>,
) -> Result<Json<GroupTasksResponse>, AppError> {
    let task_group_id = Uuid::parse_str(&task_group_id_text).map_err(|_| AppError::NotFound)?;
    let group_tasks = FindGroupTasks::new(&app_state.db_pool)
        .execute(
            current_session.user_id,
            task_group_id,
            &request_time_zone,
            UtcInstant::now(),
        )
        .await?;
    Ok(Json(group_tasks.into()))
}
