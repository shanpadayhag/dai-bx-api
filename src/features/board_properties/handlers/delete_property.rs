use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::request_time_zone::RequestTimeZone;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::board_properties::actions::delete_board_property::DeleteBoardProperty;
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::throttling::entities::write_session::WriteSession;

pub async fn delete_property(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    request_time_zone: RequestTimeZone,
    Path(property_id_text): Path<String>,
) -> Result<Json<BoardResponse>, AppError> {
    let property_id = Uuid::parse_str(&property_id_text).map_err(|_| AppError::NotFound)?;
    let board = DeleteBoardProperty::new(&app_state.db_pool)
        .execute(
            write_session.user_id,
            property_id,
            &request_time_zone,
            UtcInstant::now(),
        )
        .await?;
    Ok(Json(board.into()))
}
