use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::cards::actions::archive_card::ArchiveCard;
use crate::features::cards::responses::card_workflow_response::ArchivedStatusResponse;
use crate::features::throttling::entities::write_session::WriteSession;

pub async fn post_card_archive(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(card_id_text): Path<String>,
) -> Result<Json<ArchivedStatusResponse>, AppError> {
    let card_id = Uuid::parse_str(&card_id_text).map_err(|_| AppError::NotFound)?;
    ArchiveCard::new(&app_state.db_pool)
        .execute(write_session.user_id, card_id, UtcInstant::now())
        .await?;
    Ok(Json(ArchivedStatusResponse { status: "archived" }))
}
