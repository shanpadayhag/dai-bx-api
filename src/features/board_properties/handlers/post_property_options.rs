use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::board_properties::actions::create_property_option::CreatePropertyOption;
use crate::features::board_properties::entities::board_property::validate_option_draft;
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
pub struct PostPropertyOptionsRequest {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

pub async fn post_property_options(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(property_id_text): Path<String>,
    Json(post_request): Json<PostPropertyOptionsRequest>,
) -> Result<Json<BoardResponse>, AppError> {
    let property_id = Uuid::parse_str(&property_id_text).map_err(|_| AppError::NotFound)?;
    let option_draft = validate_option_draft(&post_request.name, post_request.color.as_deref())?;
    let board = CreatePropertyOption::new(&app_state.db_pool)
        .execute(write_session.user_id, property_id, &option_draft)
        .await?;
    Ok(Json(board.into()))
}
