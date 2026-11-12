use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::request_time_zone::RequestTimeZone;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::cards::actions::find_card_detail::FindCardDetail;
use crate::features::cards::responses::card_detail_response::CardDetailResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

pub async fn get_card(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    request_time_zone: RequestTimeZone,
    Path(card_id_text): Path<String>,
) -> Result<Json<CardDetailResponse>, AppError> {
    let card_id = Uuid::parse_str(&card_id_text).map_err(|_| AppError::NotFound)?;
    let card_detail = FindCardDetail::new(&app_state.db_pool)
        .execute(
            current_session.user_id,
            card_id,
            &request_time_zone,
            UtcInstant::now(),
        )
        .await?;
    Ok(Json(card_detail.into()))
}
