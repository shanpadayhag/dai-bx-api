use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::board_lists::actions::export_board_list::ExportBoardList;
use crate::features::board_lists::responses::list_export_response::ListExportResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

pub async fn get_list_export(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
    Path(list_id_text): Path<String>,
) -> Result<Json<ListExportResponse>, AppError> {
    let list_id = Uuid::parse_str(&list_id_text).map_err(|_| AppError::NotFound)?;
    let list_export = ExportBoardList::new(&app_state.db_pool)
        .execute(current_session.user_id, list_id)
        .await?;
    Ok(Json(list_export.into()))
}
