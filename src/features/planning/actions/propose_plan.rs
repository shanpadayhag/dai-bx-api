use std::collections::{HashMap, HashSet};

use jiff::tz::TimeZone;
use sqlx::PgPool;
use uuid::Uuid;

use crate::core::request_time_zone::RequestTimeZone;
use crate::core::utc_instant::UtcInstant;
use crate::core::wall_clock::{WallClockWindow, parse_wall_clock_time};
use crate::features::account::actions::find_time_tracking_settings::FindTimeTrackingSettings;
use crate::features::account::entities::time_tracking_settings::TimeTrackingSettings;
use crate::features::calendar_events::actions::list_calendar_events::ListCalendarEvents;
use crate::features::calendar_events::entities::calendar_event::EventRange;
use crate::features::planning::data_sources::postgres::planning_source::PostgresPlanningSource;
use crate::features::planning::entities::plan_request::PlanRequest;
use crate::features::planning::entities::planner::{
    Plan, PlanCandidate, Planner, UnplacedReason, UnplacedTask,
};
use crate::features::planning::entities::planning_inputs::{
    EstimateTask, EstimateTree, PlanRange, TimeSpan, WeekWindows, block_minutes, busy_span,
    clip_spans, effective_due, intersect_spans, merge_spans, plan_range, week_spans,
    week_windows_from,
};
use crate::features::planning::entities::planning_task::PlanningTask;
use crate::features::planning::error::PlanningError;
use crate::features::task_groups::actions::find_group_rules::FindGroupRules;
use crate::features::task_groups::entities::uncheck_cutoffs::UncheckCutoffs;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanProposal {
    pub plan_range: PlanRange,
    pub plan: Plan,
}

struct PlanningContext<'a> {
    time_zone: &'a TimeZone,
    plan_range: PlanRange,
    default_spans: &'a [TimeSpan],
    show_spans_by_group: &'a HashMap<Uuid, Vec<TimeSpan>>,
    uncheck_cutoffs: &'a UncheckCutoffs,
    left_out_task_ids: &'a HashSet<Uuid>,
}

pub struct ProposePlan<'a> {
    db_pool: &'a PgPool,
    find_time_tracking_settings: FindTimeTrackingSettings,
    find_group_rules: FindGroupRules,
    postgres_planning_source: PostgresPlanningSource,
}

fn working_hours_window(time_tracking_settings: &TimeTrackingSettings) -> Option<WallClockWindow> {
    WallClockWindow::new(
        parse_wall_clock_time(&time_tracking_settings.working_hours_start)?,
        parse_wall_clock_time(&time_tracking_settings.working_hours_end)?,
    )
}

fn unplaced(planning_task: &PlanningTask, reason: UnplacedReason) -> UnplacedTask {
    UnplacedTask {
        task_id: planning_task.id,
        task_name: planning_task.name.clone(),
        group_name: planning_task.group_name.clone(),
        reason,
    }
}

fn collect_group_candidates(
    group_tasks: &[PlanningTask],
    planning_context: &PlanningContext<'_>,
    candidates: &mut Vec<PlanCandidate>,
    left_unplaced: &mut Vec<UnplacedTask>,
) {
    let estimate_tasks: Vec<EstimateTask> = group_tasks
        .iter()
        .map(|planning_task| EstimateTask {
            id: planning_task.id,
            parent_id: planning_task.parent_id,
            estimate_minutes: planning_task.estimate_minutes,
            is_done: planning_context
                .uncheck_cutoffs
                .is_done(planning_task.group_id, planning_task.completed_at),
        })
        .collect();
    let estimate_tree = EstimateTree::new(&estimate_tasks);
    for (planning_task, estimate_task) in group_tasks.iter().zip(&estimate_tasks) {
        if planning_task.parent_id.is_some()
            || estimate_task.is_done
            || planning_context
                .left_out_task_ids
                .contains(&planning_task.id)
        {
            continue;
        }
        if planning_task.is_running {
            left_unplaced.push(unplaced(planning_task, UnplacedReason::Running));
            continue;
        }
        if planning_task.has_planned_block {
            left_unplaced.push(unplaced(planning_task, UnplacedReason::AlreadyPlanned));
            continue;
        }
        let Some(estimate_minutes) = estimate_tree.effective_minutes(planning_task.id) else {
            left_unplaced.push(unplaced(planning_task, UnplacedReason::NoEstimate));
            continue;
        };
        let task_spans = match planning_task.planning.plan_window {
            Some(plan_window) => week_spans(
                &[Some(plan_window); 7],
                planning_context.time_zone,
                planning_context.plan_range,
            ),
            None => planning_context.default_spans.to_vec(),
        };
        let allowed_spans = match planning_context
            .show_spans_by_group
            .get(&planning_task.group_id)
        {
            Some(show_spans) => intersect_spans(&task_spans, show_spans),
            None => task_spans,
        };
        candidates.push(PlanCandidate {
            task_id: planning_task.id,
            task_name: planning_task.name.clone(),
            group_id: planning_task.group_id,
            group_name: planning_task.group_name.clone(),
            group_created_at: planning_task.group_created_at,
            position: planning_task.position,
            priority: planning_task.planning.priority,
            due: effective_due(
                planning_task.planning.due,
                planning_task.card_due,
                planning_context.time_zone,
            ),
            block_minutes: block_minutes(estimate_minutes),
            allowed_spans,
        });
    }
}

impl<'a> ProposePlan<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            find_time_tracking_settings: FindTimeTrackingSettings::new(),
            find_group_rules: FindGroupRules::new(),
            postgres_planning_source: PostgresPlanningSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        plan_request: &PlanRequest,
        request_time_zone: &RequestTimeZone,
        now: UtcInstant,
    ) -> Result<PlanProposal, PlanningError> {
        let (time_tracking_settings, group_rules_listing, planning_tasks) = tokio::try_join!(
            async {
                Ok::<_, PlanningError>(
                    self.find_time_tracking_settings
                        .execute(self.db_pool, user_id)
                        .await?,
                )
            },
            async {
                Ok::<_, PlanningError>(self.find_group_rules.execute(self.db_pool, user_id).await?)
            },
            async {
                Ok::<_, PlanningError>(
                    self.postgres_planning_source
                        .list_planning_tasks(
                            self.db_pool,
                            user_id,
                            plan_request.group_ids.as_deref(),
                            now,
                        )
                        .await?,
                )
            },
        )?;
        if let Some(requested_group_ids) = &plan_request.group_ids {
            let known_group_ids: HashSet<Uuid> = group_rules_listing
                .group_rules
                .iter()
                .map(|(task_group_id, _)| *task_group_id)
                .collect();
            if !requested_group_ids
                .iter()
                .all(|group_id| known_group_ids.contains(group_id))
            {
                return Err(PlanningError::Invalid(String::from(
                    "a group in groupIds isn't one of yours",
                )));
            }
        }
        let time_zone = request_time_zone
            .calendar_time_zone(group_rules_listing.calendar_time_zone_name.as_deref());
        let week_windows: WeekWindows = week_windows_from(
            time_tracking_settings.planning_hours.as_deref(),
            working_hours_window(&time_tracking_settings),
        );
        let plan_range = plan_range(now, plan_request.day_count, &week_windows, &time_zone)
            .ok_or_else(|| PlanningError::Invalid(String::from("the range is out of bounds")))?;
        let calendar_range = ListCalendarEvents::new(self.db_pool)
            .execute(
                user_id,
                EventRange {
                    from: plan_range.plan_start.plus_days(-1),
                    to: plan_range.range_end.plus_days(1),
                },
            )
            .await?;
        let busy_spans = merge_spans(
            calendar_range
                .events
                .iter()
                .filter_map(|calendar_event| {
                    busy_span(
                        calendar_event.starts_at,
                        calendar_event.ends_at,
                        plan_range.plan_start,
                    )
                })
                .chain(
                    plan_request
                        .pinned_blocks
                        .iter()
                        .map(|pinned_block| pinned_block.span),
                )
                .collect(),
        );
        let group_timetables = group_rules_listing.work_out_timetables(request_time_zone, now);
        let uncheck_cutoffs = UncheckCutoffs::from_timetables(
            group_timetables
                .iter()
                .map(|(task_group_id, _, group_timetable)| (*task_group_id, group_timetable)),
        );
        let show_spans_by_group: HashMap<Uuid, Vec<TimeSpan>> = group_timetables
            .iter()
            .filter_map(|(task_group_id, group_rules, _)| {
                let show_rule = group_rules.show_rule.as_ref()?;
                let show_spans = show_rule
                    .spans_between(plan_range.plan_start, plan_range.range_end, &time_zone)
                    .into_iter()
                    .filter_map(|(span_start, span_end)| TimeSpan::new(span_start, span_end))
                    .collect();
                Some((
                    *task_group_id,
                    clip_spans(&merge_spans(show_spans), plan_range),
                ))
            })
            .collect();
        let default_spans = week_spans(&week_windows, &time_zone, plan_range);
        let left_out_task_ids: HashSet<Uuid> = plan_request
            .excluded_task_ids
            .iter()
            .copied()
            .chain(
                plan_request
                    .pinned_blocks
                    .iter()
                    .map(|pinned_block| pinned_block.task_id),
            )
            .collect();
        let planning_context = PlanningContext {
            time_zone: &time_zone,
            plan_range,
            default_spans: &default_spans,
            show_spans_by_group: &show_spans_by_group,
            uncheck_cutoffs: &uncheck_cutoffs,
            left_out_task_ids: &left_out_task_ids,
        };
        let mut candidates = Vec::new();
        let mut left_unplaced = Vec::new();
        for group_tasks in planning_tasks
            .chunk_by(|left_task, right_task| left_task.group_id == right_task.group_id)
        {
            collect_group_candidates(
                group_tasks,
                &planning_context,
                &mut candidates,
                &mut left_unplaced,
            );
        }
        let mut plan = Planner::plan(candidates, busy_spans);
        left_unplaced.append(&mut plan.unplaced);
        plan.unplaced = left_unplaced;
        Ok(PlanProposal { plan_range, plan })
    }
}
