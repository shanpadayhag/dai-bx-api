use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::activity::actions::list_activity::ListActivity;
use crate::features::activity::entities::activity_page::ActivityQuery;
use crate::features::activity::responses::activity_entry_response::ActivityPageResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

#[derive(Deserialize)]
pub struct GetActivityQuery {
    pub since: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<String>,
}

pub async fn get_activity(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    Query(activity_query): Query<GetActivityQuery>,
) -> Result<Json<ActivityPageResponse>, AppError> {
    let activity_query = ActivityQuery::parse(
        activity_query.since.as_deref(),
        activity_query.cursor.as_deref(),
        activity_query.limit.as_deref(),
    )?;
    let activity_page = ListActivity::new(&app_state.db_pool)
        .execute(current_session.user_id, activity_query)
        .await?;
    Ok(Json(ActivityPageResponse::try_from(activity_page)?))
}
