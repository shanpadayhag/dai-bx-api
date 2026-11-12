use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use serde::de::IgnoredAny;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::json_field::deserialize_present_field;
use crate::core::request_time_zone::RequestTimeZone;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::board_properties::actions::rename_board_property::RenameBoardProperty;
use crate::features::board_properties::entities::board_property::validate_property_name;
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
pub struct PatchPropertyRequest {
    pub name: String,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub kind: Option<IgnoredAny>,
}

pub async fn patch_property(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    request_time_zone: RequestTimeZone,
    Path(property_id_text): Path<String>,
    Json(patch_request): Json<PatchPropertyRequest>,
) -> Result<Json<BoardResponse>, AppError> {
    let property_id = Uuid::parse_str(&property_id_text).map_err(|_| AppError::NotFound)?;
    if patch_request.kind.is_some() {
        return Err(AppError::Validation(String::from(
            "a property's kind can't change",
        )));
    }
    let property_name = validate_property_name(&patch_request.name)?;
    let board = RenameBoardProperty::new(&app_state.db_pool)
        .execute(
            write_session.user_id,
            property_id,
            &property_name,
            &request_time_zone,
            UtcInstant::now(),
        )
        .await?;
    Ok(Json(board.into()))
}
