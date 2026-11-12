use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;

use crate::core::error::AppError;
use crate::core::json_field::deserialize_present_field;
use crate::core::recurrence::RepeatRule;
use crate::core::request_time_zone::RequestTimeZone;
use crate::core::state::AppState;
use crate::core::utc_instant::UtcInstant;
use crate::features::task_groups::actions::create_scheduled_task_group::CreateScheduledTaskGroup;
use crate::features::task_groups::entities::group_rules::{
    GroupRuleChanges, ShowRuleInput, UncheckRuleInput, build_show_window, parse_wall_clock_time,
};
use crate::features::task_groups::entities::task_group::validate_task_group_name;
use crate::features::task_groups::error::TaskGroupError;
use crate::features::task_groups::responses::task_group_response::TaskGroupResponse;
use crate::features::throttling::entities::write_session::WriteSession;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowsRequest {
    repeat: RepeatRule,
    window_start: Option<String>,
    window_end: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnchecksRequest {
    repeat: RepeatRule,
    time: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskGroupRequest {
    name: String,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    shows: Option<Option<ShowsRequest>>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    unchecks: Option<Option<UnchecksRequest>>,
}

impl ShowsRequest {
    fn into_input(self) -> Result<ShowRuleInput, TaskGroupError> {
        Ok(ShowRuleInput {
            window: build_show_window(self.window_start.as_deref(), self.window_end.as_deref())?,
            repeat_rule: self.repeat,
        })
    }
}

impl UnchecksRequest {
    fn into_input(self) -> Result<UncheckRuleInput, TaskGroupError> {
        Ok(UncheckRuleInput {
            time: parse_wall_clock_time(&self.time)?,
            repeat_rule: self.repeat,
        })
    }
}

impl TaskGroupRequest {
    pub fn into_name_and_changes(self) -> Result<(String, GroupRuleChanges), TaskGroupError> {
        let task_group_name = validate_task_group_name(&self.name)?;
        let group_rule_changes = GroupRuleChanges {
            show_change: self
                .shows
                .map(|shows| shows.map(ShowsRequest::into_input).transpose())
                .transpose()?,
            uncheck_change: self
                .unchecks
                .map(|unchecks| unchecks.map(UnchecksRequest::into_input).transpose())
                .transpose()?,
        };
        Ok((task_group_name, group_rule_changes))
    }
}

pub async fn post_task_groups(
    State(app_state): State<AppState>,
    write_session: WriteSession,
    request_time_zone: RequestTimeZone,
    Json(task_group_request): Json<TaskGroupRequest>,
) -> Result<(StatusCode, Json<TaskGroupResponse>), AppError> {
    let (task_group_name, group_rule_changes) = task_group_request.into_name_and_changes()?;
    let created_task_group = CreateScheduledTaskGroup::new(&app_state.db_pool)
        .execute(
            write_session.user_id,
            &task_group_name,
            &group_rule_changes,
            &request_time_zone,
            UtcInstant::now(),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(created_task_group.into())))
}
