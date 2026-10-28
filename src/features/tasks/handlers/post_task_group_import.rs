use axum::Json;
use axum::body::{Body, to_bytes};
use axum::extract::State;
use axum::http::StatusCode;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::features::tasks::actions::import_group::ImportGroup;
use crate::features::tasks::entities::group_import::{MAX_IMPORT_BYTES, parse_group_import};
use crate::features::tasks::error::TaskError;
use crate::features::tasks::responses::group_tasks_response::GroupTasksResponse;
use crate::features::throttling::entities::write_session::WriteSession;

pub async fn post_task_group_import(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    import_body: Body,
) -> Result<(StatusCode, Json<GroupTasksResponse>), AppError> {
    let import_bytes = to_bytes(import_body, MAX_IMPORT_BYTES)
        .await
        .map_err(|_| AppError::PayloadTooLarge(String::from("the file is larger than 1 MB")))?;
    let group_import = parse_group_import(&import_bytes).map_err(TaskError::from)?;
    let group_tasks = ImportGroup::new(&app_state.db_pool)
        .execute(write_session.user_id, &group_import)
        .await?;
    Ok((StatusCode::CREATED, Json(group_tasks.into())))
}
