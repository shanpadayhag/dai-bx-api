use axum::Json;
use axum::extract::State;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::calendar_events::actions::list_running_items::ListRunningItems;
use crate::features::sessions::entities::current_session::CurrentSession;
use crate::features::tracking::responses::running_item_response::RunningItemResponse;

pub async fn get_running_items(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
) -> Result<Json<Vec<RunningItemResponse>>, AppError> {
    let running_items = ListRunningItems::new(&app_state.db_pool)
        .execute(current_session.user_id)
        .await?;
    Ok(Json(
        running_items
            .into_iter()
            .map(RunningItemResponse::from)
            .collect(),
    ))
}
