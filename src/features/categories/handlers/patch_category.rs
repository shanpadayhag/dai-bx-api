use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::categories::actions::update_category::UpdateCategory;
use crate::features::categories::entities::category::{
    CategoryChanges, validate_category_color, validate_category_name,
};
use crate::features::categories::responses::category_response::CategoryResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
pub struct PatchCategoryRequest {
    pub name: Option<String>,
    pub color: Option<String>,
}

pub async fn patch_category(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    Path(category_id_text): Path<String>,
    Json(patch_request): Json<PatchCategoryRequest>,
) -> Result<Json<CategoryResponse>, AppError> {
    let category_id = Uuid::parse_str(&category_id_text).map_err(|_| AppError::NotFound)?;
    let category_changes = CategoryChanges {
        name: patch_request
            .name
            .as_deref()
            .map(validate_category_name)
            .transpose()?,
        color: patch_request
            .color
            .as_deref()
            .map(validate_category_color)
            .transpose()?,
    };
    let updated_category = UpdateCategory::new()
        .execute(
            &app_state.db_pool,
            write_session.user_id,
            category_id,
            &category_changes,
        )
        .await?;
    Ok(Json(updated_category.into()))
}
