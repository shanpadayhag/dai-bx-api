use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::planning::entities::planning_inputs::{TimeSpan, align_to_quarter_hour};
use crate::features::planning::entities::planning_order::PlanningOrder;
use crate::features::tasks::entities::task::TaskPriority;

pub const CANDIDATE_LIMIT: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanCandidate {
    pub task_id: Uuid,
    pub task_name: String,
    pub group_id: Uuid,
    pub group_name: String,
    pub group_created_at: UtcInstant,
    pub position: i32,
    pub priority: TaskPriority,
    pub due: Option<UtcInstant>,
    pub block_minutes: i64,
    pub allowed_spans: Vec<TimeSpan>,
}

impl PlanCandidate {
    fn group_order_key(&self) -> (UtcInstant, Uuid, i32, Uuid) {
        (
            self.group_created_at,
            self.group_id,
            self.position,
            self.task_id,
        )
    }

    fn plan_order_key(&self) -> PlanningOrder {
        PlanningOrder {
            due: self.due,
            priority: self.priority,
            group_created_at: self.group_created_at,
            group_id: self.group_id,
            position: self.position,
            task_id: self.task_id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnplacedReason {
    NoEstimate,
    AlreadyPlanned,
    Running,
    DoesNotFit,
    TooMany,
}

impl UnplacedReason {
    pub fn as_str(self) -> &'static str {
        match self {
            UnplacedReason::NoEstimate => "noEstimate",
            UnplacedReason::AlreadyPlanned => "alreadyPlanned",
            UnplacedReason::Running => "running",
            UnplacedReason::DoesNotFit => "doesNotFit",
            UnplacedReason::TooMany => "tooMany",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnplacedTask {
    pub task_id: Uuid,
    pub task_name: String,
    pub group_name: String,
    pub reason: UnplacedReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedBlock {
    pub task_id: Uuid,
    pub task_name: String,
    pub group_id: Uuid,
    pub group_name: String,
    pub starts_at: UtcInstant,
    pub ends_at: UtcInstant,
    pub due: Option<UtcInstant>,
    pub is_late: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub blocks: Vec<PlannedBlock>,
    pub unplaced: Vec<UnplacedTask>,
}

fn unplaced(candidate: PlanCandidate, reason: UnplacedReason) -> UnplacedTask {
    UnplacedTask {
        task_id: candidate.task_id,
        task_name: candidate.task_name,
        group_name: candidate.group_name,
        reason,
    }
}

fn earliest_fit(
    allowed_spans: &[TimeSpan],
    busy_spans: &[TimeSpan],
    block_minutes: i64,
) -> Option<TimeSpan> {
    for allowed_span in allowed_spans {
        let mut block_start = align_to_quarter_hour(allowed_span.start);
        loop {
            let block_end = block_start.plus_minutes(block_minutes);
            if block_end > allowed_span.end {
                break;
            }
            let next_busy_index =
                busy_spans.partition_point(|busy_span| busy_span.end <= block_start);
            match busy_spans.get(next_busy_index) {
                Some(busy_span) if busy_span.start < block_end => {
                    block_start = align_to_quarter_hour(busy_span.end);
                }
                _ => {
                    return Some(TimeSpan {
                        start: block_start,
                        end: block_end,
                    });
                }
            }
        }
    }
    None
}

pub struct Planner;

impl Planner {
    pub fn plan(mut candidates: Vec<PlanCandidate>, mut busy_spans: Vec<TimeSpan>) -> Plan {
        let mut plan = Plan::default();
        if candidates.len() > CANDIDATE_LIMIT {
            candidates.sort_by_key(PlanCandidate::group_order_key);
            plan.unplaced.extend(
                candidates
                    .split_off(CANDIDATE_LIMIT)
                    .into_iter()
                    .map(|candidate| unplaced(candidate, UnplacedReason::TooMany)),
            );
        }
        candidates.sort_by_key(PlanCandidate::plan_order_key);
        let mut does_not_fit = Vec::new();
        for candidate in candidates {
            let Some(block_span) = earliest_fit(
                &candidate.allowed_spans,
                &busy_spans,
                candidate.block_minutes,
            ) else {
                does_not_fit.push(unplaced(candidate, UnplacedReason::DoesNotFit));
                continue;
            };
            let insert_index =
                busy_spans.partition_point(|busy_span| busy_span.end <= block_span.start);
            busy_spans.insert(insert_index, block_span);
            plan.blocks.push(PlannedBlock {
                is_late: candidate.due.is_some_and(|due| block_span.end > due),
                task_id: candidate.task_id,
                task_name: candidate.task_name,
                group_id: candidate.group_id,
                group_name: candidate.group_name,
                starts_at: block_span.start,
                ends_at: block_span.end,
                due: candidate.due,
            });
        }
        does_not_fit.append(&mut plan.unplaced);
        plan.unplaced = does_not_fit;
        plan
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(instant_text: &str) -> UtcInstant {
        UtcInstant::parse(&format!("2026-11-21T{instant_text}:00Z")).expect("a valid time")
    }

    fn span(start_text: &str, end_text: &str) -> TimeSpan {
        TimeSpan::new(at(start_text), at(end_text)).expect("a span")
    }

    fn candidate(task_name: &str, block_minutes: i64) -> PlanCandidate {
        PlanCandidate {
            task_id: Uuid::from_u128(1),
            task_name: task_name.to_owned(),
            group_id: Uuid::from_u128(100),
            group_name: String::from("Today"),
            group_created_at: at("00:00"),
            position: 0,
            priority: TaskPriority::Normal,
            due: None,
            block_minutes,
            allowed_spans: vec![span("09:00", "12:00")],
        }
    }

    fn block_times(plan: &Plan) -> Vec<(&str, UtcInstant, UtcInstant)> {
        plan.blocks
            .iter()
            .map(|block| (block.task_name.as_str(), block.starts_at, block.ends_at))
            .collect()
    }

    fn block_names(plan: &Plan) -> Vec<&str> {
        plan.blocks
            .iter()
            .map(|block| block.task_name.as_str())
            .collect()
    }

    #[test]
    fn orders_by_due_then_priority_then_group_age_then_position_then_id() {
        let due_soon = PlanCandidate {
            due: Some(at("18:00")),
            ..candidate("due soon", 15)
        };
        let due_later = PlanCandidate {
            due: Some(at("20:00")),
            priority: TaskPriority::High,
            ..candidate("due later", 15)
        };
        let high = PlanCandidate {
            priority: TaskPriority::High,
            ..candidate("high", 15)
        };
        let older_group = PlanCandidate {
            group_created_at: at("00:00"),
            position: 5,
            ..candidate("older group", 15)
        };
        let newer_group = PlanCandidate {
            group_created_at: at("01:00"),
            ..candidate("newer group", 15)
        };
        let tie_first = PlanCandidate {
            task_id: Uuid::from_u128(2),
            position: 6,
            ..candidate("tie first", 15)
        };
        let tie_second = PlanCandidate {
            task_id: Uuid::from_u128(3),
            position: 6,
            ..candidate("tie second", 15)
        };

        let plan = Planner::plan(
            vec![
                tie_second,
                newer_group,
                high,
                tie_first,
                older_group,
                due_later,
                due_soon,
            ],
            Vec::new(),
        );

        assert_eq!(
            block_names(&plan),
            vec![
                "due soon",
                "due later",
                "high",
                "older group",
                "tie first",
                "tie second",
                "newer group",
            ]
        );
        assert_eq!(plan.blocks[0].starts_at, at("09:00"));
        assert_eq!(plan.blocks[1].starts_at, at("09:15"));
    }

    #[test]
    fn takes_the_earliest_quarter_hour_gap_that_fits() {
        let first_task = PlanCandidate {
            allowed_spans: vec![span("09:05", "12:00")],
            ..candidate("first", 30)
        };
        let after_meeting = PlanCandidate {
            task_id: Uuid::from_u128(2),
            ..candidate("after meeting", 60)
        };
        let too_long = PlanCandidate {
            task_id: Uuid::from_u128(3),
            ..candidate("too long", 120)
        };

        let plan = Planner::plan(
            vec![first_task, after_meeting, too_long],
            vec![span("09:50", "10:10"), span("11:20", "12:00")],
        );

        assert_eq!(
            block_times(&plan),
            vec![
                ("first", at("09:15"), at("09:45")),
                ("after meeting", at("10:15"), at("11:15")),
            ]
        );
        assert_eq!(plan.unplaced[0].task_name, "too long");
        assert_eq!(plan.unplaced[0].reason, UnplacedReason::DoesNotFit);
    }

    #[test]
    fn moves_on_to_the_next_allowed_span() {
        let split_task = PlanCandidate {
            allowed_spans: vec![span("09:00", "09:30"), span("14:00", "16:00")],
            ..candidate("split", 45)
        };

        let plan = Planner::plan(vec![split_task], vec![span("14:00", "14:20")]);

        assert_eq!(
            block_times(&plan),
            vec![("split", at("14:30"), at("15:15"))]
        );
    }

    #[test]
    fn marks_a_block_that_ends_after_its_due_as_late() {
        let late_task = PlanCandidate {
            due: Some(at("10:00")),
            ..candidate("late", 60)
        };
        let on_time = PlanCandidate {
            task_id: Uuid::from_u128(2),
            due: Some(at("12:00")),
            ..candidate("on time", 60)
        };

        let plan = Planner::plan(vec![late_task, on_time], vec![span("09:00", "09:30")]);

        assert_eq!(
            block_times(&plan),
            vec![
                ("late", at("09:30"), at("10:30")),
                ("on time", at("10:30"), at("11:30")),
            ]
        );
        assert!(plan.blocks[0].is_late);
        assert!(!plan.blocks[1].is_late);
    }

    #[test]
    fn reports_candidates_over_the_limit_as_too_many_from_the_newest_groups() {
        let candidates: Vec<PlanCandidate> = (0..=CANDIDATE_LIMIT)
            .map(|candidate_index| {
                let candidate_number = u128::try_from(candidate_index).expect("a small index");
                PlanCandidate {
                    task_id: Uuid::from_u128(candidate_number + 1),
                    group_created_at: UtcInstant::from_unix_millis(
                        i64::try_from(candidate_index).expect("a small index"),
                    ),
                    allowed_spans: Vec::new(),
                    ..candidate(&format!("task {candidate_index}"), 15)
                }
            })
            .rev()
            .collect();

        let plan = Planner::plan(candidates, Vec::new());

        let too_many: Vec<&UnplacedTask> = plan
            .unplaced
            .iter()
            .filter(|unplaced_task| unplaced_task.reason == UnplacedReason::TooMany)
            .collect();
        assert_eq!(too_many.len(), 1);
        assert_eq!(too_many[0].task_name, "task 500");
        assert_eq!(plan.unplaced.len(), CANDIDATE_LIMIT + 1);
    }

    #[test]
    fn the_same_inputs_give_the_same_plan() {
        let candidates: Vec<PlanCandidate> = (0..20)
            .map(|candidate_index| PlanCandidate {
                task_id: Uuid::from_u128(candidate_index + 1),
                ..candidate(&format!("task {candidate_index}"), 15)
            })
            .collect();
        let busy_spans = vec![span("10:00", "10:40")];

        assert_eq!(
            Planner::plan(candidates.clone(), busy_spans.clone()),
            Planner::plan(candidates, busy_spans)
        );
    }
}
