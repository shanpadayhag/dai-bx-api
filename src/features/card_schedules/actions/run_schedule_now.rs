use sqlx::PgPool;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::card_schedules::actions::lock_schedule::LockSchedule;
use crate::features::card_schedules::actions::run_schedule::RunSchedule;
use crate::features::card_schedules::entities::card_schedule::CardSchedule;
use crate::features::card_schedules::entities::schedule_run::{RunRequest, RunTiming};
use crate::features::card_schedules::error::ScheduleError;

pub struct RunScheduleNow<'a> {
    db_pool: &'a PgPool,
    lock_schedule: LockSchedule,
    run_schedule: RunSchedule,
}

impl<'a> RunScheduleNow<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_schedule: LockSchedule::new(),
            run_schedule: RunSchedule::new(),
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        schedule_id: Uuid,
        now: UtcInstant,
    ) -> Result<CardSchedule, ScheduleError> {
        let mut run_transaction = self.db_pool.begin().await?;
        let locked_schedule = self
            .lock_schedule
            .execute(&mut run_transaction, user_id, schedule_id)
            .await?;
        let card_schedule = self
            .run_schedule
            .execute(
                &mut run_transaction,
                user_id,
                locked_schedule,
                RunRequest {
                    occurrence_at: now,
                    timing: RunTiming::ByHand,
                    now,
                },
            )
            .await?;
        run_transaction.commit().await?;
        Ok(card_schedule)
    }
}
