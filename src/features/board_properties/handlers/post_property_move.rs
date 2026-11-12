use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::request_time_zone::RequestTimeZone;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::board_properties::actions::move_board_property::MoveBoardProperty;
use crate::features::board_properties::entities::board_property::validate_position;
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
pub struct PostMoveRequest {
    pub position: i64,
}

pub async fn post_property_move(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    request_time_zone: RequestTimeZone,
    Path(property_id_text): Path<String>,
    Json(move_request): Json<PostMoveRequest>,
) -> Result<Json<BoardResponse>, AppError> {
    let property_id = Uuid::parse_str(&property_id_text).map_err(|_| AppError::NotFound)?;
    let requested_position = validate_position(move_request.position)?;
    let board = MoveBoardProperty::new(&app_state.db_pool)
        .execute(
            write_session.user_id,
            property_id,
            requested_position,
            &request_time_zone,
            UtcInstant::now(),
        )
        .await?;
    Ok(Json(board.into()))
}
