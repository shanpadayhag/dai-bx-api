use sqlx::PgPool;

use crate::core::utc_instant::UtcInstant;
use crate::features::card_schedules::actions::lock_schedule::LockSchedule;
use crate::features::card_schedules::actions::run_schedule::RunSchedule;
use crate::features::card_schedules::data_sources::postgres::card_schedule_source::PostgresCardScheduleSource;
use crate::features::card_schedules::entities::card_schedule::DueSchedule;
use crate::features::card_schedules::entities::schedule_run::{
    DUE_SCHEDULES_PER_CALL, DueRunTally, RunRequest, RunTiming,
};
use crate::features::card_schedules::error::ScheduleError;

pub struct RunDueSchedules<'a> {
    db_pool: &'a PgPool,
    lock_schedule: LockSchedule,
    run_schedule: RunSchedule,
    postgres_card_schedule_source: PostgresCardScheduleSource,
}

impl<'a> RunDueSchedules<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_schedule: LockSchedule::new(),
            run_schedule: RunSchedule::new(),
            postgres_card_schedule_source: PostgresCardScheduleSource,
        }
    }

    pub async fn execute(&self, now: UtcInstant) -> Result<DueRunTally, ScheduleError> {
        let due_schedules = self
            .postgres_card_schedule_source
            .list_due(self.db_pool, now, DUE_SCHEDULES_PER_CALL)
            .await?;
        let mut due_run_tally = DueRunTally::default();
        for due_schedule in due_schedules {
            match self.run_due_schedule(due_schedule, now).await {
                Ok(true) => due_run_tally.ran_count += 1,
                Ok(false) => {}
                Err(run_error) => {
                    due_run_tally.failed_count += 1;
                    tracing::error!(
                        schedule_id = %due_schedule.schedule_id,
                        error = %run_error,
                        "a due schedule failed to run"
                    );
                }
            }
        }
        Ok(due_run_tally)
    }

    async fn run_due_schedule(
        &self,
        due_schedule: DueSchedule,
        now: UtcInstant,
    ) -> Result<bool, ScheduleError> {
        let mut run_transaction = self.db_pool.begin().await?;
        let locked_schedule = match self
            .lock_schedule
            .execute(
                &mut run_transaction,
                due_schedule.user_id,
                due_schedule.schedule_id,
            )
            .await
        {
            Err(ScheduleError::NotFound) => return Ok(false),
            locked_result => locked_result?,
        };
        if !locked_schedule.is_due(now) {
            return Ok(false);
        }
        let Some(occurrence_at) = locked_schedule.timing.latest_run_at_or_before(now) else {
            return Ok(false);
        };
        self.run_schedule
            .execute(
                &mut run_transaction,
                due_schedule.user_id,
                locked_schedule,
                RunRequest {
                    occurrence_at,
                    timing: RunTiming::for_occurrence(occurrence_at, now),
                    now,
                },
            )
            .await?;
        run_transaction.commit().await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::features::card_schedules::data_sources::postgres::card_schedule_source::tests::{
        instant, new_schedule, seed_card, seed_schedule, seed_user_id, weekly_settings,
    };

    async fn next_run_of(db_pool: &PgPool, user_id: Uuid, schedule_id: Uuid) -> Option<UtcInstant> {
        PostgresCardScheduleSource
            .find_locked(db_pool, user_id, schedule_id)
            .await
            .expect("the read should run")
            .and_then(|card_schedule| card_schedule.next_run_at)
    }

    #[sqlx::test]
    async fn runs_what_is_due_and_leaves_the_rest(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let due_schedule_id = seed_schedule(
            &db_pool,
            &new_schedule(&seeded_card, weekly_settings(seeded_card.to_do_list_id)),
        )
        .await;
        let later_schedule_id = seed_schedule(
            &db_pool,
            &new_schedule(&seeded_card, weekly_settings(seeded_card.doing_list_id)),
        )
        .await;
        PostgresCardScheduleSource
            .set_next_run_at(
                &db_pool,
                due_schedule_id,
                Some(instant("2030-01-07T10:00:00Z")),
            )
            .await
            .expect("the next run should be set");
        PostgresCardScheduleSource
            .set_next_run_at(
                &db_pool,
                later_schedule_id,
                Some(instant("2030-01-14T10:00:00Z")),
            )
            .await
            .expect("the next run should be set");

        let due_run_tally = RunDueSchedules::new(&db_pool)
            .execute(instant("2030-01-07T10:00:30Z"))
            .await
            .expect("the call should run");

        assert_eq!(
            due_run_tally,
            DueRunTally {
                ran_count: 1,
                failed_count: 0,
            }
        );
        assert_eq!(
            next_run_of(&db_pool, user_id, due_schedule_id).await,
            Some(instant("2030-01-14T10:00:00Z"))
        );
        assert_eq!(
            next_run_of(&db_pool, user_id, later_schedule_id).await,
            Some(instant("2030-01-14T10:00:00Z"))
        );
    }
}
