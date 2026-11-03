use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::board_properties::actions::create_board_property::CreateBoardProperty;
use crate::features::board_properties::entities::board_property::{
    OptionDraft, validate_new_property_options, validate_option_draft, validate_property_name,
};
use crate::features::board_properties::handlers::post_property_options::PostPropertyOptionsRequest;
use crate::features::boards::entities::board::BoardPropertyKind;
use crate::features::boards::responses::board_response::BoardResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
pub struct PostBoardPropertiesRequest {
    pub name: String,
    pub kind: BoardPropertyKind,
    #[serde(default)]
    pub options: Vec<PostPropertyOptionsRequest>,
}

pub async fn post_board_properties(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(board_id_text): Path<String>,
    Json(post_request): Json<PostBoardPropertiesRequest>,
) -> Result<Json<BoardResponse>, AppError> {
    let board_id = Uuid::parse_str(&board_id_text).map_err(|_| AppError::NotFound)?;
    let property_name = validate_property_name(&post_request.name)?;
    let option_drafts = post_request
        .options
        .iter()
        .map(|option_request| {
            validate_option_draft(&option_request.name, option_request.color.as_deref())
        })
        .collect::<Result<Vec<OptionDraft>, _>>()?;
    validate_new_property_options(post_request.kind, &option_drafts)?;
    let board = CreateBoardProperty::new(&app_state.db_pool)
        .execute(
            write_session.user_id,
            board_id,
            &property_name,
            post_request.kind,
            &option_drafts,
        )
        .await?;
    Ok(Json(board.into()))
}
