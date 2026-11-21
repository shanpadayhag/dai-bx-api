use std::collections::HashMap;

use jiff::civil::Time;
use jiff::tz::TimeZone;
use uuid::Uuid;

use crate::core::recurrence::{to_instant, to_local};
use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::entities::calendar_event::{
    CalendarEvent, CalendarEventKind,
};
use crate::features::planning::entities::planning_inputs::{
    EstimateTask, EstimateTree, effective_due,
};
use crate::features::planning::entities::planning_order::PlanningOrder;
use crate::features::planning::entities::planning_task::PlanningTask;
use crate::features::task_groups::entities::group_timetable::GroupTimetable;
use crate::features::task_groups::entities::uncheck_cutoffs::UncheckCutoffs;
use crate::features::tasks::entities::task::{TaskDue, TaskPriority};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpNextBlock {
    pub event_id: Uuid,
    pub starts_at: UtcInstant,
    pub ends_at: UtcInstant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpNextTask {
    pub task_id: Uuid,
    pub name: String,
    pub group_id: Uuid,
    pub group_name: String,
    pub estimate_minutes: Option<i64>,
    pub due: Option<TaskDue>,
    pub priority: TaskPriority,
    pub planned_block: Option<UpNextBlock>,
    pub is_missed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpNext {
    pub tasks: Vec<UpNextTask>,
    pub refresh_at: UtcInstant,
}

pub struct UpNextInputs<'a> {
    pub planning_tasks: &'a [PlanningTask],
    pub group_timetables: &'a HashMap<Uuid, GroupTimetable>,
    pub uncheck_cutoffs: &'a UncheckCutoffs,
    pub today_events: &'a [CalendarEvent],
    pub time_zone: &'a TimeZone,
    pub now: UtcInstant,
    pub day_end: UtcInstant,
}

pub fn local_day_span(now: UtcInstant, time_zone: &TimeZone) -> Option<(UtcInstant, UtcInstant)> {
    let today = to_local(now, time_zone)?.date();
    let day_start = to_instant(today.to_datetime(Time::midnight()), time_zone)?;
    let day_end = to_instant(
        today.tomorrow().ok()?.to_datetime(Time::midnight()),
        time_zone,
    )?;
    Some((day_start, day_end))
}

fn earliest_unstarted_blocks(today_events: &[CalendarEvent]) -> HashMap<Uuid, UpNextBlock> {
    let mut latest_tracked_start_by_task: HashMap<Uuid, UtcInstant> = HashMap::new();
    for tracked_event in today_events
        .iter()
        .filter(|calendar_event| calendar_event.kind == CalendarEventKind::Tracked)
    {
        if let Some(task_id) = tracked_event.task_id() {
            latest_tracked_start_by_task
                .entry(task_id)
                .and_modify(|latest_start| {
                    *latest_start = (*latest_start).max(tracked_event.starts_at);
                })
                .or_insert(tracked_event.starts_at);
        }
    }
    let mut earliest_block_by_task: HashMap<Uuid, UpNextBlock> = HashMap::new();
    for planned_event in today_events
        .iter()
        .filter(|calendar_event| calendar_event.kind == CalendarEventKind::Planned)
    {
        let (Some(task_id), Some(ends_at)) = (planned_event.task_id(), planned_event.ends_at)
        else {
            continue;
        };
        let is_started = latest_tracked_start_by_task
            .get(&task_id)
            .is_some_and(|latest_start| *latest_start >= planned_event.starts_at);
        if is_started {
            continue;
        }
        let planned_block = UpNextBlock {
            event_id: planned_event.id,
            starts_at: planned_event.starts_at,
            ends_at,
        };
        earliest_block_by_task
            .entry(task_id)
            .and_modify(|earliest_block| {
                if planned_block.starts_at < earliest_block.starts_at {
                    *earliest_block = planned_block;
                }
            })
            .or_insert(planned_block);
    }
    earliest_block_by_task
}

fn next_refresh_at(
    inputs: &UpNextInputs<'_>,
    planned_blocks: &HashMap<Uuid, UpNextBlock>,
) -> UtcInstant {
    let group_changes = inputs
        .group_timetables
        .values()
        .flat_map(|group_timetable| {
            let show_change = if group_timetable.is_due_now {
                group_timetable.due_until
            } else {
                group_timetable.next_shows_at
            };
            [show_change, group_timetable.next_uncheck_at]
        });
    let block_starts = planned_blocks
        .values()
        .map(|planned_block| Some(planned_block.starts_at));
    group_changes
        .chain(block_starts)
        .flatten()
        .filter(|change_at| *change_at > inputs.now)
        .fold(inputs.day_end, UtcInstant::min)
}

impl UpNext {
    pub fn work_out(inputs: &UpNextInputs<'_>) -> Self {
        let planned_blocks = earliest_unstarted_blocks(inputs.today_events);
        let mut planned_tasks: Vec<(UtcInstant, PlanningOrder, UpNextTask)> = Vec::new();
        let mut due_now_tasks: Vec<(PlanningOrder, UpNextTask)> = Vec::new();
        for group_tasks in inputs
            .planning_tasks
            .chunk_by(|left_task, right_task| left_task.group_id == right_task.group_id)
        {
            let estimate_tasks: Vec<EstimateTask> = group_tasks
                .iter()
                .map(|planning_task| EstimateTask {
                    id: planning_task.id,
                    parent_id: planning_task.parent_id,
                    estimate_minutes: planning_task.estimate_minutes,
                    is_done: inputs
                        .uncheck_cutoffs
                        .is_done(planning_task.group_id, planning_task.completed_at),
                })
                .collect();
            let estimate_tree = EstimateTree::new(&estimate_tasks);
            for (planning_task, estimate_task) in group_tasks.iter().zip(&estimate_tasks) {
                if planning_task.parent_id.is_some()
                    || estimate_task.is_done
                    || planning_task.is_running
                {
                    continue;
                }
                let planned_block = planned_blocks.get(&planning_task.id).copied();
                let is_group_due_now = inputs
                    .group_timetables
                    .get(&planning_task.group_id)
                    .is_none_or(|group_timetable| group_timetable.is_due_now);
                if planned_block.is_none() && !is_group_due_now {
                    continue;
                }
                let planning_order = PlanningOrder {
                    due: effective_due(
                        planning_task.planning.due,
                        planning_task.card_due,
                        inputs.time_zone,
                    ),
                    priority: planning_task.planning.priority,
                    group_created_at: planning_task.group_created_at,
                    group_id: planning_task.group_id,
                    position: planning_task.position,
                    task_id: planning_task.id,
                };
                let up_next_task = UpNextTask {
                    task_id: planning_task.id,
                    name: planning_task.name.clone(),
                    group_id: planning_task.group_id,
                    group_name: planning_task.group_name.clone(),
                    estimate_minutes: estimate_tree.effective_minutes(planning_task.id),
                    due: planning_task.planning.due.or(planning_task.card_due),
                    priority: planning_task.planning.priority,
                    planned_block,
                    is_missed: planned_block
                        .is_some_and(|planned_block| planned_block.starts_at < inputs.now),
                };
                match planned_block {
                    Some(planned_block) => {
                        planned_tasks.push((planned_block.starts_at, planning_order, up_next_task));
                    }
                    None => due_now_tasks.push((planning_order, up_next_task)),
                }
            }
        }
        planned_tasks.sort_by_key(|(starts_at, planning_order, _)| (*starts_at, *planning_order));
        due_now_tasks.sort_by_key(|(planning_order, _)| *planning_order);
        let refresh_at = next_refresh_at(inputs, &planned_blocks);
        Self {
            tasks: planned_tasks
                .into_iter()
                .map(|(_, _, up_next_task)| up_next_task)
                .chain(
                    due_now_tasks
                        .into_iter()
                        .map(|(_, up_next_task)| up_next_task),
                )
                .collect(),
            refresh_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::calendar_events::entities::calendar_event::CalendarEventSource;
    use crate::features::tasks::entities::task::TaskPlanning;

    const NOW_MILLIS: i64 = 10 * 3_600_000;
    const DAY_END_MILLIS: i64 = 24 * 3_600_000;

    fn group_id(group_number: u128) -> Uuid {
        Uuid::from_u128(1_000 + group_number)
    }

    fn task(task_number: u128, group_number: u128, position: i32) -> PlanningTask {
        PlanningTask {
            id: Uuid::from_u128(task_number),
            parent_id: None,
            group_id: group_id(group_number),
            group_name: format!("Group {group_number}"),
            group_created_at: UtcInstant::from_unix_millis(group_number as i64),
            position,
            name: format!("Task {task_number}"),
            estimate_minutes: Some(30),
            completed_at: None,
            planning: TaskPlanning::default(),
            card_due: None,
            has_planned_block: false,
            is_running: false,
        }
    }

    fn event(
        kind: CalendarEventKind,
        task_number: u128,
        starts_minute: i64,
        ends_minute: Option<i64>,
    ) -> CalendarEvent {
        CalendarEvent {
            id: Uuid::now_v7(),
            kind,
            title: String::from("Block"),
            category_id: None,
            source: Some(CalendarEventSource::Task {
                task_id: Uuid::from_u128(task_number),
                group_id: group_id(1),
            }),
            starts_at: UtcInstant::from_unix_millis(starts_minute * 60_000),
            ends_at: ends_minute.map(|minute| UtcInstant::from_unix_millis(minute * 60_000)),
            finished_at: None,
            last_answered_at: None,
            time_zone: None,
            series: None,
            linked_task: None,
            is_schedule_block: false,
        }
    }

    fn work_out(
        planning_tasks: &[PlanningTask],
        group_timetables: &HashMap<Uuid, GroupTimetable>,
        today_events: &[CalendarEvent],
    ) -> UpNext {
        UpNext::work_out(&UpNextInputs {
            planning_tasks,
            group_timetables,
            uncheck_cutoffs: &UncheckCutoffs::default(),
            today_events,
            time_zone: &TimeZone::UTC,
            now: UtcInstant::from_unix_millis(NOW_MILLIS),
            day_end: UtcInstant::from_unix_millis(DAY_END_MILLIS),
        })
    }

    fn task_numbers(up_next: &UpNext) -> Vec<u128> {
        up_next
            .tasks
            .iter()
            .map(|up_next_task| up_next_task.task_id.as_u128())
            .collect()
    }

    fn not_due_now() -> GroupTimetable {
        GroupTimetable {
            is_due_now: false,
            due_until: None,
            next_shows_at: Some(UtcInstant::from_unix_millis(NOW_MILLIS + 3_600_000)),
            last_uncheck_at: None,
            next_uncheck_at: None,
        }
    }

    #[test]
    fn lists_planned_tasks_by_block_start_before_the_rest_in_planning_order() {
        let mut high_task = task(3, 1, 2);
        high_task.planning.priority = TaskPriority::High;
        let planning_tasks = vec![task(1, 1, 0), task(2, 1, 1), high_task, task(4, 1, 3)];
        let today_events = vec![
            event(CalendarEventKind::Planned, 4, 900, Some(930)),
            event(CalendarEventKind::Planned, 2, 660, Some(690)),
        ];

        let up_next = work_out(&planning_tasks, &HashMap::new(), &today_events);

        assert_eq!(task_numbers(&up_next), vec![2, 4, 3, 1]);
        assert_eq!(
            up_next.tasks[0]
                .planned_block
                .map(|block| block.starts_at.unix_millis()),
            Some(660 * 60_000)
        );
        assert!(!up_next.tasks[0].is_missed);
    }

    #[test]
    fn marks_a_planned_block_whose_start_has_passed_as_missed() {
        let planning_tasks = vec![task(1, 1, 0)];
        let today_events = vec![event(CalendarEventKind::Planned, 1, 540, Some(570))];

        let up_next = work_out(&planning_tasks, &HashMap::new(), &today_events);

        assert!(up_next.tasks[0].is_missed);
    }

    #[test]
    fn drops_a_block_once_its_task_was_started_after_it_began() {
        let planning_tasks = vec![task(1, 1, 0)];
        let today_events = vec![
            event(CalendarEventKind::Planned, 1, 540, Some(570)),
            event(CalendarEventKind::Tracked, 1, 545, Some(560)),
        ];

        let up_next = work_out(&planning_tasks, &HashMap::new(), &today_events);

        assert_eq!(task_numbers(&up_next), vec![1]);
        assert_eq!(up_next.tasks[0].planned_block, None);
    }

    #[test]
    fn leaves_out_running_done_and_subtasks() {
        let mut running_task = task(1, 1, 0);
        running_task.is_running = true;
        let mut done_task = task(2, 1, 1);
        done_task.completed_at = Some(UtcInstant::from_unix_millis(0));
        let mut subtask = task(4, 1, 0);
        subtask.parent_id = Some(Uuid::from_u128(3));
        let planning_tasks = vec![running_task, done_task, task(3, 1, 2), subtask];

        let up_next = work_out(&planning_tasks, &HashMap::new(), &[]);

        assert_eq!(task_numbers(&up_next), vec![3]);
    }

    #[test]
    fn leaves_out_groups_not_due_now_unless_the_task_is_planned_today() {
        let planning_tasks = vec![task(1, 1, 0), task(2, 2, 0), task(3, 2, 1)];
        let group_timetables = HashMap::from([(group_id(2), not_due_now())]);
        let today_events = vec![event(CalendarEventKind::Planned, 3, 700, Some(730))];

        let up_next = work_out(&planning_tasks, &group_timetables, &today_events);

        assert_eq!(task_numbers(&up_next), vec![3, 1]);
    }

    #[test]
    fn sums_open_subtask_estimates_when_the_task_has_none() {
        let mut parent_task = task(1, 1, 0);
        parent_task.estimate_minutes = None;
        let mut first_subtask = task(2, 1, 0);
        first_subtask.parent_id = Some(Uuid::from_u128(1));
        first_subtask.estimate_minutes = Some(20);
        let mut second_subtask = task(3, 1, 1);
        second_subtask.parent_id = Some(Uuid::from_u128(1));
        second_subtask.estimate_minutes = Some(25);

        let up_next = work_out(
            &[parent_task, first_subtask, second_subtask],
            &HashMap::new(),
            &[],
        );

        assert_eq!(up_next.tasks[0].estimate_minutes, Some(45));
    }

    #[test]
    fn refreshes_at_the_earliest_of_midnight_a_group_change_and_the_next_block() {
        let planning_tasks = vec![task(1, 1, 0), task(2, 2, 0)];
        let today_events = vec![event(CalendarEventKind::Planned, 1, 1_200, Some(1_230))];

        let at_midnight = work_out(&planning_tasks, &HashMap::new(), &[]);
        let at_block = work_out(&planning_tasks, &HashMap::new(), &today_events);
        let at_group_change = work_out(
            &planning_tasks,
            &HashMap::from([(group_id(2), not_due_now())]),
            &today_events,
        );

        assert_eq!(at_midnight.refresh_at.unix_millis(), DAY_END_MILLIS);
        assert_eq!(at_block.refresh_at.unix_millis(), 1_200 * 60_000);
        assert_eq!(
            at_group_change.refresh_at.unix_millis(),
            NOW_MILLIS + 3_600_000
        );
    }

    #[test]
    fn a_local_day_runs_from_midnight_to_midnight_in_the_zone() {
        let manila = TimeZone::get("Asia/Manila").expect("the zone should exist");
        let now = UtcInstant::parse("2026-11-21T20:30:00Z").expect("instant should parse");

        let (day_start, day_end) = local_day_span(now, &manila).expect("the day should resolve");

        assert_eq!(day_start.to_iso_text(), "2026-11-21T16:00:00.000Z");
        assert_eq!(day_end.to_iso_text(), "2026-11-22T16:00:00.000Z");
    }
}
