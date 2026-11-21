use axum::Json;
use axum::extract::State;

use crate::core::error::AppError;
use crate::core::request_time_zone::RequestTimeZone;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::planning::actions::find_up_next::FindUpNext;
use crate::features::planning::responses::up_next_response::UpNextResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

pub async fn get_up_next(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    request_time_zone: RequestTimeZone,
) -> Result<Json<UpNextResponse>, AppError> {
    let up_next = FindUpNext::new(&app_state.db_pool)
        .execute(
            current_session.user_id,
            &request_time_zone,
            UtcInstant::now(),
        )
        .await?;
    Ok(Json(up_next.into()))
}
