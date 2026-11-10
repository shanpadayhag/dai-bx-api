use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::throttling::entities::write_session::WriteSession;
use crate::features::tracking::actions::start_card::StartCard;
use crate::features::tracking::responses::running_item_response::RunningItemResponse;

pub async fn post_card_start(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(card_id_text): Path<String>,
) -> Result<(StatusCode, Json<RunningItemResponse>), AppError> {
    let card_id = Uuid::parse_str(&card_id_text).map_err(|_| AppError::NotFound)?;
    let running_item = StartCard::new(&app_state.db_pool)
        .execute(write_session.user_id, card_id, UtcInstant::now())
        .await?;
    Ok((StatusCode::CREATED, Json(running_item.into())))
}
