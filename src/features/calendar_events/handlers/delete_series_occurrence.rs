use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::calendar_events::actions::delete_series_occurrence::DeleteSeriesOccurrence;
use crate::features::calendar_events::entities::event_series::{
    OccurrenceScope, parse_occurrence_path,
};
use crate::features::calendar_events::responses::deleted_response::DeletedResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
pub struct DeleteSeriesOccurrenceQuery {
    scope: Option<String>,
}

pub async fn delete_series_occurrence(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path((series_id_text, original_start_text)): Path<(String, String)>,
    Query(delete_query): Query<DeleteSeriesOccurrenceQuery>,
) -> Result<Json<DeletedResponse>, AppError> {
    let occurrence_scope =
        OccurrenceScope::parse(delete_query.scope.as_deref().unwrap_or_default())?;
    let (series_id, original_start) = parse_occurrence_path(&series_id_text, &original_start_text)?;
    DeleteSeriesOccurrence::new(&app_state.db_pool)
        .execute(
            write_session.user_id,
            series_id,
            original_start,
            occurrence_scope,
        )
        .await?;
    Ok(Json(DeletedResponse::deleted()))
}
