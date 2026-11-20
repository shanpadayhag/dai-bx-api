use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::planning::entities::planning_inputs::{QUARTER_HOUR_MINUTES, TimeSpan};
use crate::features::planning::error::PlanningError;

pub const PROPOSAL_DAY_COUNTS: [i64; 2] = [1, 7];
pub const PLAN_BLOCK_LIMIT: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanBlock {
    pub task_id: Uuid,
    pub span: TimeSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRequest {
    pub day_count: i64,
    pub group_ids: Option<Vec<Uuid>>,
    pub excluded_task_ids: Vec<Uuid>,
    pub pinned_blocks: Vec<PlanBlock>,
}

fn invalid(invalid_message: &str) -> PlanningError {
    PlanningError::Invalid(invalid_message.to_owned())
}

pub fn validate_day_count(day_count: i64) -> Result<i64, PlanningError> {
    PROPOSAL_DAY_COUNTS
        .contains(&day_count)
        .then_some(day_count)
        .ok_or_else(|| invalid("days is 1 or 7"))
}

pub fn validate_plan_block(
    task_id: Uuid,
    starts_at: UtcInstant,
    ends_at: UtcInstant,
) -> Result<PlanBlock, PlanningError> {
    let span =
        TimeSpan::new(starts_at, ends_at).ok_or_else(|| invalid("a block ends after it starts"))?;
    if ends_at < starts_at.plus_minutes(QUARTER_HOUR_MINUTES) {
        return Err(invalid("a block lasts at least 15 minutes"));
    }
    Ok(PlanBlock { task_id, span })
}

pub fn validate_pinned_count(pinned_count: usize) -> Result<(), PlanningError> {
    if pinned_count > PLAN_BLOCK_LIMIT {
        return Err(invalid("a plan keeps at most 200 pinned blocks"));
    }
    Ok(())
}

pub fn validate_accepted_count(accepted_count: usize) -> Result<(), PlanningError> {
    if accepted_count == 0 || accepted_count > PLAN_BLOCK_LIMIT {
        return Err(invalid("accept 1 to 200 blocks"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(instant_text: &str) -> UtcInstant {
        UtcInstant::parse(instant_text).expect("a valid time")
    }

    #[test]
    fn days_is_one_or_seven() {
        assert_eq!(validate_day_count(1).ok(), Some(1));
        assert_eq!(validate_day_count(7).ok(), Some(7));
        assert!(validate_day_count(3).is_err());
    }

    #[test]
    fn a_block_ends_after_it_starts_and_lasts_a_quarter_hour() {
        let task_id = Uuid::now_v7();

        assert!(
            validate_plan_block(
                task_id,
                at("2026-11-21T09:00:00Z"),
                at("2026-11-21T09:15:00Z")
            )
            .is_ok()
        );
        assert!(
            validate_plan_block(
                task_id,
                at("2026-11-21T09:00:00Z"),
                at("2026-11-21T09:14:00Z")
            )
            .is_err()
        );
        assert!(
            validate_plan_block(
                task_id,
                at("2026-11-21T09:00:00Z"),
                at("2026-11-21T08:00:00Z")
            )
            .is_err()
        );
    }

    #[test]
    fn block_counts_stay_within_two_hundred() {
        assert!(validate_pinned_count(0).is_ok());
        assert!(validate_pinned_count(200).is_ok());
        assert!(validate_pinned_count(201).is_err());
        assert!(validate_accepted_count(0).is_err());
        assert!(validate_accepted_count(1).is_ok());
        assert!(validate_accepted_count(201).is_err());
    }
}
