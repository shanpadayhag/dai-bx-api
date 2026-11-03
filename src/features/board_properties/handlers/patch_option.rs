use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::json_field::deserialize_present_field;
use crate::core::state::AppState;
use crate::features::board_properties::actions::update_property_option::UpdatePropertyOption;
use crate::features::board_properties::entities::board_property::{
    OptionChanges, validate_option_color, validate_option_name,
};
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
pub struct PatchOptionRequest {
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub name: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub color: Option<Option<String>>,
}

pub async fn patch_option(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(option_id_text): Path<String>,
    Json(patch_request): Json<PatchOptionRequest>,
) -> Result<Json<BoardResponse>, AppError> {
    let option_id = Uuid::parse_str(&option_id_text).map_err(|_| AppError::NotFound)?;
    let option_changes = OptionChanges {
        name: patch_request
            .name
            .map(|name| validate_option_name(name.as_deref().unwrap_or_default()))
            .transpose()?,
        color_change: patch_request
            .color
            .map(|color| color.as_deref().map(validate_option_color).transpose())
            .transpose()?,
    };
    let board = UpdatePropertyOption::new(&app_state.db_pool)
        .execute(write_session.user_id, option_id, &option_changes)
        .await?;
    Ok(Json(board.into()))
}
