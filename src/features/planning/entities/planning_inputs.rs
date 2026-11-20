use std::collections::HashMap;

use jiff::civil::{Date, Time};
use jiff::tz::TimeZone;
use uuid::Uuid;

use crate::core::recurrence::{to_instant, to_local};
use crate::core::utc_instant::UtcInstant;
use crate::core::wall_clock::WallClockWindow;
use crate::features::account::entities::time_tracking_settings::{PlanningDay, PlanningWeek};
use crate::features::tasks::entities::task::TaskDue;

pub const QUARTER_HOUR_MINUTES: i64 = 15;
const QUARTER_HOUR_MILLIS: i64 = QUARTER_HOUR_MINUTES * 60_000;
const DAYS_PER_WEEK: usize = 7;

pub type WeekWindows = [Option<WallClockWindow>; DAYS_PER_WEEK];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimeSpan {
    pub start: UtcInstant,
    pub end: UtcInstant,
}

impl TimeSpan {
    pub fn new(start: UtcInstant, end: UtcInstant) -> Option<Self> {
        (start < end).then_some(Self { start, end })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanRange {
    pub plan_start: UtcInstant,
    pub range_end: UtcInstant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EstimateTask {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub estimate_minutes: Option<i32>,
    pub is_done: bool,
}

pub struct EstimateTree<'a> {
    tasks_by_id: HashMap<Uuid, &'a EstimateTask>,
    children_by_parent: HashMap<Uuid, Vec<&'a EstimateTask>>,
}

impl<'a> EstimateTree<'a> {
    pub fn new(group_tasks: &'a [EstimateTask]) -> Self {
        let mut children_by_parent: HashMap<Uuid, Vec<&'a EstimateTask>> = HashMap::new();
        for group_task in group_tasks {
            if let Some(parent_id) = group_task.parent_id {
                children_by_parent
                    .entry(parent_id)
                    .or_default()
                    .push(group_task);
            }
        }
        Self {
            tasks_by_id: group_tasks
                .iter()
                .map(|group_task| (group_task.id, group_task))
                .collect(),
            children_by_parent,
        }
    }

    pub fn effective_minutes(&self, task_id: Uuid) -> Option<i64> {
        self.tasks_by_id
            .get(&task_id)
            .and_then(|estimate_task| self.minutes_of(estimate_task))
    }

    fn minutes_of(&self, estimate_task: &EstimateTask) -> Option<i64> {
        if let Some(own_minutes) = estimate_task.estimate_minutes {
            return Some(i64::from(own_minutes));
        }
        self.children_by_parent
            .get(&estimate_task.id)?
            .iter()
            .filter(|child_task| !child_task.is_done)
            .filter_map(|child_task| self.minutes_of(child_task))
            .reduce(|minute_total, child_minutes| minute_total + child_minutes)
    }
}

pub fn block_minutes(estimate_minutes: i64) -> i64 {
    let quarter_count = (estimate_minutes + QUARTER_HOUR_MINUTES - 1) / QUARTER_HOUR_MINUTES;
    (quarter_count * QUARTER_HOUR_MINUTES).max(QUARTER_HOUR_MINUTES)
}

pub fn effective_due(
    task_due: Option<TaskDue>,
    card_due: Option<TaskDue>,
    time_zone: &TimeZone,
) -> Option<UtcInstant> {
    let chosen_due = task_due.or(card_due)?;
    let due_moment = match chosen_due.time {
        Some(due_time) => chosen_due.date.to_datetime(due_time),
        None => chosen_due
            .date
            .tomorrow()
            .ok()?
            .to_datetime(Time::midnight()),
    };
    to_instant(due_moment, time_zone)
}

pub fn align_to_quarter_hour(instant: UtcInstant) -> UtcInstant {
    let unix_millis = instant.unix_millis();
    let quarter_count = unix_millis.div_euclid(QUARTER_HOUR_MILLIS)
        + i64::from(unix_millis.rem_euclid(QUARTER_HOUR_MILLIS) != 0);
    UtcInstant::from_unix_millis(quarter_count * QUARTER_HOUR_MILLIS)
}

fn weekday_index(local_date: Date) -> usize {
    usize::try_from(local_date.weekday().to_monday_zero_offset()).unwrap_or_default()
}

fn window_span(
    local_date: Date,
    window: WallClockWindow,
    time_zone: &TimeZone,
) -> Option<TimeSpan> {
    let end_date = if window.crosses_midnight() {
        local_date.tomorrow().ok()?
    } else {
        local_date
    };
    TimeSpan::new(
        to_instant(local_date.to_datetime(window.start), time_zone)?,
        to_instant(end_date.to_datetime(window.end), time_zone)?,
    )
}

pub fn plan_range(
    now: UtcInstant,
    day_count: i64,
    week_windows: &WeekWindows,
    time_zone: &TimeZone,
) -> Option<PlanRange> {
    let today = to_local(now, time_zone)?.date();
    let last_date = today
        .checked_add(jiff::Span::new().days(day_count - 1))
        .ok()?;
    let range_end = match week_windows[weekday_index(last_date)] {
        Some(last_window) => window_span(last_date, last_window, time_zone)?.end,
        None => to_instant(
            last_date.tomorrow().ok()?.to_datetime(Time::midnight()),
            time_zone,
        )?,
    };
    let plan_start = align_to_quarter_hour(now);
    Some(PlanRange {
        plan_start,
        range_end: range_end.max(plan_start),
    })
}

pub fn week_windows_from(
    planning_week: Option<&PlanningWeek>,
    working_hours_window: Option<WallClockWindow>,
) -> WeekWindows {
    let mut week_windows = [working_hours_window; DAYS_PER_WEEK];
    if let Some(planning_week) = planning_week {
        for (day_window, planning_day) in week_windows.iter_mut().zip(planning_week) {
            *day_window = planning_day.as_ref().and_then(PlanningDay::window);
        }
    }
    week_windows
}

pub fn clip_spans(spans: &[TimeSpan], plan_range: PlanRange) -> Vec<TimeSpan> {
    spans
        .iter()
        .filter_map(|span| {
            TimeSpan::new(
                span.start.max(plan_range.plan_start),
                span.end.min(plan_range.range_end),
            )
        })
        .collect()
}

pub fn week_spans(
    week_windows: &WeekWindows,
    time_zone: &TimeZone,
    plan_range: PlanRange,
) -> Vec<TimeSpan> {
    let (Some(first_local), Some(last_local)) = (
        to_local(plan_range.plan_start, time_zone),
        to_local(plan_range.range_end, time_zone),
    ) else {
        return Vec::new();
    };
    let Ok(mut local_date) = first_local.date().yesterday() else {
        return Vec::new();
    };
    let mut spans = Vec::new();
    while local_date <= last_local.date() {
        if let Some(span) = week_windows[weekday_index(local_date)]
            .and_then(|window| window_span(local_date, window, time_zone))
        {
            spans.push(span);
        }
        let Ok(next_date) = local_date.tomorrow() else {
            break;
        };
        local_date = next_date;
    }
    clip_spans(&spans, plan_range)
}

pub fn intersect_spans(left_spans: &[TimeSpan], right_spans: &[TimeSpan]) -> Vec<TimeSpan> {
    let mut shared_spans = Vec::new();
    let (mut left_index, mut right_index) = (0, 0);
    while let (Some(left_span), Some(right_span)) =
        (left_spans.get(left_index), right_spans.get(right_index))
    {
        if let Some(shared_span) = TimeSpan::new(
            left_span.start.max(right_span.start),
            left_span.end.min(right_span.end),
        ) {
            shared_spans.push(shared_span);
        }
        if left_span.end < right_span.end {
            left_index += 1;
        } else {
            right_index += 1;
        }
    }
    shared_spans
}

pub fn merge_spans(mut spans: Vec<TimeSpan>) -> Vec<TimeSpan> {
    spans.sort_unstable();
    let mut merged_spans: Vec<TimeSpan> = Vec::with_capacity(spans.len());
    for span in spans {
        match merged_spans.last_mut() {
            Some(last_span) if span.start <= last_span.end => {
                last_span.end = last_span.end.max(span.end);
            }
            _ => merged_spans.push(span),
        }
    }
    merged_spans
}

pub fn busy_span(
    starts_at: UtcInstant,
    ends_at: Option<UtcInstant>,
    plan_start: UtcInstant,
) -> Option<TimeSpan> {
    TimeSpan::new(starts_at, ends_at.unwrap_or(plan_start))
}

#[cfg(test)]
mod tests {
    use jiff::civil::{date, time};

    use super::*;
    use crate::core::recurrence::parse_time_zone;

    fn zone(zone_name: &str) -> TimeZone {
        parse_time_zone(zone_name).expect("a known zone")
    }

    fn at(local_text: &str, time_zone: &TimeZone) -> UtcInstant {
        to_instant(local_text.parse().expect("a local time"), time_zone).expect("an instant")
    }

    fn span(start_text: &str, end_text: &str, time_zone: &TimeZone) -> TimeSpan {
        TimeSpan::new(at(start_text, time_zone), at(end_text, time_zone)).expect("a span")
    }

    fn window(start_hour: i8, end_hour: i8) -> Option<WallClockWindow> {
        WallClockWindow::new(time(start_hour, 0, 0, 0), time(end_hour, 0, 0, 0))
    }

    fn estimate_task(
        id: Uuid,
        parent_id: Option<Uuid>,
        estimate_minutes: Option<i32>,
        is_done: bool,
    ) -> EstimateTask {
        EstimateTask {
            id,
            parent_id,
            estimate_minutes,
            is_done,
        }
    }

    #[test]
    fn an_estimate_is_the_task_s_own_or_the_sum_of_its_open_subtasks() {
        let (own_id, parent_id, child_id, grandchild_id, ticked_id, reopened_id, bare_id) = (
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
        );
        let group_tasks = vec![
            estimate_task(own_id, None, Some(40), false),
            estimate_task(parent_id, None, None, false),
            estimate_task(child_id, Some(parent_id), None, false),
            estimate_task(grandchild_id, Some(child_id), Some(25), false),
            estimate_task(ticked_id, Some(parent_id), Some(60), true),
            estimate_task(reopened_id, Some(parent_id), Some(10), false),
            estimate_task(bare_id, None, None, false),
        ];
        let estimate_tree = EstimateTree::new(&group_tasks);

        assert_eq!(estimate_tree.effective_minutes(own_id), Some(40));
        assert_eq!(estimate_tree.effective_minutes(parent_id), Some(35));
        assert_eq!(estimate_tree.effective_minutes(bare_id), None);
        assert_eq!(estimate_tree.effective_minutes(Uuid::now_v7()), None);
    }

    #[test]
    fn a_block_is_at_least_a_quarter_hour_and_rounds_up() {
        for (estimate_minutes, expected_minutes) in [(1, 15), (15, 15), (16, 30), (50, 60)] {
            assert_eq!(block_minutes(estimate_minutes), expected_minutes);
        }
    }

    #[test]
    fn the_due_is_the_task_s_else_the_card_s_and_a_bare_date_ends_at_midnight() {
        let manila = zone("Asia/Manila");
        let task_due = TaskDue {
            date: date(2026, 11, 21),
            time: Some(time(17, 0, 0, 0)),
        };
        let card_due = TaskDue {
            date: date(2026, 11, 25),
            time: None,
        };

        assert_eq!(
            effective_due(Some(task_due), Some(card_due), &manila),
            Some(at("2026-11-21T17:00", &manila))
        );
        assert_eq!(
            effective_due(None, Some(card_due), &manila),
            Some(at("2026-11-26T00:00", &manila))
        );
        assert_eq!(effective_due(None, None, &manila), None);
    }

    fn weekday_hours() -> WeekWindows {
        [
            window(9, 12),
            window(9, 12),
            None,
            window(9, 12),
            window(9, 12),
            window(22, 2),
            None,
        ]
    }

    #[test]
    fn planning_hours_replace_working_hours_day_by_day() {
        let working_hours = window(16, 2);
        let planning_week: PlanningWeek = vec![
            Some(PlanningDay {
                start: String::from("08:00"),
                end: String::from("12:00"),
            }),
            None,
            None,
            None,
            None,
            None,
            None,
        ];

        let following_working_hours = week_windows_from(None, working_hours);
        let custom_week = week_windows_from(Some(&planning_week), working_hours);

        assert_eq!(following_working_hours, [working_hours; 7]);
        assert_eq!(custom_week[0], window(8, 12));
        assert_eq!(custom_week[1..], [None; 6]);
    }

    #[test]
    fn the_range_ends_with_the_last_day_s_window_or_its_midnight() {
        let utc = zone("UTC");
        let thursday_morning = at("2026-11-19T08:07", &utc);

        let one_day = plan_range(thursday_morning, 1, &weekday_hours(), &utc).expect("a range");
        let seven_days = plan_range(thursday_morning, 7, &weekday_hours(), &utc).expect("a range");
        let till_saturday =
            plan_range(thursday_morning, 3, &weekday_hours(), &utc).expect("a range");

        assert_eq!(one_day.plan_start, at("2026-11-19T08:15", &utc));
        assert_eq!(one_day.range_end, at("2026-11-19T12:00", &utc));
        assert_eq!(seven_days.range_end, at("2026-11-26T00:00", &utc));
        assert_eq!(till_saturday.range_end, at("2026-11-22T02:00", &utc));
    }

    #[test]
    fn planning_hours_skip_an_off_day_and_keep_a_window_that_began_yesterday() {
        let utc = zone("UTC");
        let sunday_night = at("2026-11-22T01:00", &utc);
        let plan_range = PlanRange {
            plan_start: sunday_night,
            range_end: at("2026-11-26T00:00", &utc),
        };

        let spans = week_spans(&weekday_hours(), &utc, plan_range);

        assert_eq!(
            spans,
            vec![
                span("2026-11-22T01:00", "2026-11-22T02:00", &utc),
                span("2026-11-23T09:00", "2026-11-23T12:00", &utc),
                span("2026-11-24T09:00", "2026-11-24T12:00", &utc),
            ]
        );
    }

    #[test]
    fn a_window_stays_on_the_wall_clock_across_a_dst_change() {
        let london = zone("Europe/London");
        let plan_range = PlanRange {
            plan_start: at("2026-10-24T00:00", &london),
            range_end: at("2026-10-26T00:00", &london),
        };

        let spans = week_spans(&[window(9, 10); 7], &london, plan_range);

        assert_eq!(
            spans,
            vec![
                span("2026-10-24T09:00", "2026-10-24T10:00", &london),
                span("2026-10-25T09:00", "2026-10-25T10:00", &london),
            ]
        );
        assert_eq!(
            spans[1].start.unix_millis() - spans[0].start.unix_millis(),
            25 * 60 * 60 * 1000
        );
    }

    #[test]
    fn intersecting_keeps_only_the_shared_time() {
        let utc = zone("UTC");
        let hours = vec![
            span("2026-11-21T09:00", "2026-11-21T12:00", &utc),
            span("2026-11-22T09:00", "2026-11-22T12:00", &utc),
            span("2026-11-23T09:00", "2026-11-23T12:00", &utc),
        ];
        let weekend = vec![span("2026-11-21T00:00", "2026-11-23T00:00", &utc)];

        assert_eq!(intersect_spans(&hours, &weekend), hours[..2].to_vec());
        assert_eq!(intersect_spans(&hours, &[]), Vec::new());
    }

    #[test]
    fn busy_events_merge_and_a_running_one_lasts_until_the_plan_start() {
        let utc = zone("UTC");
        let plan_start = at("2026-11-21T10:00", &utc);
        let running_span =
            busy_span(at("2026-11-21T09:30", &utc), None, plan_start).expect("a span");

        let merged = merge_spans(vec![
            span("2026-11-21T11:00", "2026-11-21T12:00", &utc),
            running_span,
            span("2026-11-21T11:30", "2026-11-21T13:00", &utc),
            span("2026-11-21T10:00", "2026-11-21T10:30", &utc),
        ]);

        assert_eq!(running_span.end, plan_start);
        assert_eq!(
            merged,
            vec![
                span("2026-11-21T09:30", "2026-11-21T10:30", &utc),
                span("2026-11-21T11:00", "2026-11-21T13:00", &utc),
            ]
        );
    }
}
