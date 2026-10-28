use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::categories::actions::create_category::CreateCategory;
use crate::features::categories::entities::category::{
    validate_category_color, validate_category_name,
};
use crate::features::categories::responses::category_response::CategoryResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
pub struct PostCategoriesRequest {
    pub name: String,
    pub color: String,
}

pub async fn post_categories(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Json(post_request): Json<PostCategoriesRequest>,
) -> Result<(StatusCode, Json<CategoryResponse>), AppError> {
    let category_name = validate_category_name(&post_request.name)?;
    let category_color = validate_category_color(&post_request.color)?;
    let created_category = CreateCategory::new()
        .execute(
            &app_state.db_pool,
            write_session.user_id,
            &category_name,
            &category_color,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(created_category.into())))
}
