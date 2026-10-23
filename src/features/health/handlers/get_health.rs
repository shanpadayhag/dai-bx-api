use axum::Json;
use axum::extract::State;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::health::actions::check_health::CheckHealth;
use crate::features::health::responses::health_response::HealthResponse;

pub async fn get_health(
    State(app_state): State<AppState>,
) -> Result<Json<HealthResponse>, AppError> {
    CheckHealth::new(&app_state.db_pool).execute().await?;
    Ok(Json(HealthResponse::healthy()))
}
