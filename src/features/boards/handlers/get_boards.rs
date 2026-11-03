use axum::Json;
use axum::extract::State;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::boards::actions::list_boards::ListBoards;
use crate::features::boards::responses::board_response::BoardSummaryResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

pub async fn get_boards(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
) -> Result<Json<Vec<BoardSummaryResponse>>, AppError> {
    let board_summaries = ListBoards::new()
        .execute(&app_state.db_pool, current_session.user_id)
        .await?;
    Ok(Json(
        board_summaries
            .into_iter()
            .map(BoardSummaryResponse::from)
            .collect(),
    ))
}
