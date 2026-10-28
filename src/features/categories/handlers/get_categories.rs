use axum::Json;
use axum::extract::State;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::categories::actions::list_categories::ListCategories;
use crate::features::categories::responses::category_response::CategoryResponse;
use crate::features::sessions::entities::current_session::CurrentSession;

pub async fn get_categories(
    State(app_state): State<AppState>,
    current_session: CurrentSession,
) -> Result<Json<Vec<CategoryResponse>>, AppError> {
    let categories = ListCategories::new()
        .execute(&app_state.db_pool, current_session.user_id)
        .await?;
    Ok(Json(
        categories.into_iter().map(CategoryResponse::from).collect(),
    ))
}
