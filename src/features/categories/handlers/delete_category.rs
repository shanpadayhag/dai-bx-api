use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::categories::actions::delete_category::DeleteCategory;
use crate::features::categories::responses::deleted_response::DeletedResponse;
use crate::features::throttling::entities::write_session::WriteSession;

pub async fn delete_category(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(category_id_text): Path<String>,
) -> Result<Json<DeletedResponse>, AppError> {
    let category_id = Uuid::parse_str(&category_id_text).map_err(|_| AppError::NotFound)?;
    DeleteCategory::new()
        .execute(&app_state.db_pool, write_session.user_id, category_id)
        .await?;
    Ok(Json(DeletedResponse::deleted()))
}
