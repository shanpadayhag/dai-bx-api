use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::search::actions::search_for::SearchFor;
use crate::features::search::entities::search_query::SearchQuery;
use crate::features::search::responses::search_response::SearchResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

#[derive(Deserialize)]
pub struct GetSearchQuery {
    pub q: Option<String>,
    pub kind: Option<String>,
    pub limit: Option<String>,
}

pub async fn get_search(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    Query(get_search_query): Query<GetSearchQuery>,
) -> Result<Json<SearchResponse>, AppError> {
    let search_query = SearchQuery::parse(
        get_search_query.q.as_deref(),
        get_search_query.kind.as_deref(),
        get_search_query.limit.as_deref(),
    )?;
    let search_results = SearchFor::new(&app_state.db_pool)
        .execute(current_session.user_id, &search_query, UtcInstant::now())
        .await?;
    Ok(Json(SearchResponse::from(search_results)))
}
