use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::core::error::AppError;
use crate::core::request_context::RequestContext;
use crate::core::state::AppState;
use crate::features::notifications::actions::remove_push_device::RemovePushDevice;
use crate::features::notifications::responses::deleted_response::DeletedResponse;
use crate::features::throttling::entities::write_session::WriteSession;

pub async fn delete_push_device(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    request_context: RequestContext,
    Path(device_id_text): Path<String>,
) -> Result<Json<DeletedResponse>, AppError> {
    let device_id = Uuid::parse_str(&device_id_text).map_err(|_| AppError::NotFound)?;
    RemovePushDevice::new(&app_state.db_pool)
        .execute(write_session.user_id, device_id, &request_context)
        .await?;
    Ok(Json(DeletedResponse::deleted()))
}
