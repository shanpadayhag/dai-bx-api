use sqlx::PgConnection;
use uuid::Uuid;

use crate::features::card_schedules::data_sources::postgres::card_schedule_source::PostgresCardScheduleSource;
use crate::features::card_schedules::data_sources::postgres::schedule_run_source::PostgresScheduleRunSource;
use crate::features::card_schedules::entities::card_schedule::CardSchedule;
use crate::features::card_schedules::entities::schedule_run::{
    NewScheduleRun, RunRequest, RunTiming,
};
use crate::features::card_schedules::error::ScheduleError;
use crate::features::cards::actions::move_card_to_list_top::MoveCardToListTop;
use crate::features::cards::entities::card_move::{CardMoveReason, MoveCause};

#[derive(Default)]
pub struct RunSchedule {
    move_card_to_list_top: MoveCardToListTop,
    postgres_card_schedule_source: PostgresCardScheduleSource,
    postgres_schedule_run_source: PostgresScheduleRunSource,
}

impl RunSchedule {
    pub fn new() -> Self {
        Self {
            move_card_to_list_top: MoveCardToListTop::new(),
            postgres_card_schedule_source: PostgresCardScheduleSource,
            postgres_schedule_run_source: PostgresScheduleRunSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        card_schedule: CardSchedule,
        run_request: RunRequest,
    ) -> Result<CardSchedule, ScheduleError> {
        let schedule_id = card_schedule.id;
        let target_list_id = card_schedule.target_list_id.ok_or_else(|| {
            ScheduleError::Invalid(String::from("Its column was deleted. Pick another."))
        })?;
        let new_run = NewScheduleRun {
            id: Uuid::now_v7(),
            user_id,
            schedule_id,
            occurrence_at: run_request.occurrence_at,
            ran_at: run_request.now,
            timing: run_request.timing,
        };
        let is_inserted = self
            .postgres_schedule_run_source
            .insert(&mut *connection, &new_run)
            .await?;
        if is_inserted {
            let is_moved = self
                .move_card_to_list_top
                .execute(
                    &mut *connection,
                    card_schedule.board_id,
                    card_schedule.card_id,
                    target_list_id,
                    &MoveCause::without_task(CardMoveReason::Schedule, run_request.now),
                )
                .await?;
            if is_moved {
                self.postgres_schedule_run_source
                    .mark_moved(&mut *connection, new_run.id)
                    .await?;
            }
        }
        if run_request.timing == RunTiming::ByHand {
            return Ok(card_schedule);
        }
        let next_run_at = card_schedule.timing.next_run_after(run_request.now);
        self.postgres_card_schedule_source
            .set_next_run_at(&mut *connection, schedule_id, next_run_at)
            .await?;
        Ok(CardSchedule {
            next_run_at,
            ..card_schedule
        })
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::core::utc_instant::UtcInstant;
    use crate::features::card_schedules::actions::lock_schedule::LockSchedule;
    use crate::features::card_schedules::data_sources::postgres::card_schedule_source::tests::{
        instant, new_schedule, seed_card, seed_schedule, seed_user_id, weekly_settings,
    };
    use crate::features::card_schedules::entities::card_schedule::{
        ScheduleSettings, ScheduleTiming,
    };

    async fn run(
        db_pool: &PgPool,
        user_id: Uuid,
        schedule_id: Uuid,
        occurrence_at: UtcInstant,
        timing: RunTiming,
        now: UtcInstant,
    ) -> CardSchedule {
        let mut run_transaction = db_pool.begin().await.expect("a transaction");
        let locked_schedule = LockSchedule::new()
            .execute(&mut run_transaction, user_id, schedule_id)
            .await
            .expect("the schedule should lock");
        let card_schedule = RunSchedule::new()
            .execute(
                &mut run_transaction,
                user_id,
                locked_schedule,
                RunRequest {
                    occurrence_at,
                    timing,
                    now,
                },
            )
            .await
            .expect("the run should succeed");
        run_transaction
            .commit()
            .await
            .expect("the commit should run");
        card_schedule
    }

    async fn run_rows(db_pool: &PgPool, schedule_id: Uuid) -> Vec<(String, bool)> {
        sqlx::query!(
            "SELECT how, moved FROM schedule_runs WHERE schedule_id = $1 ORDER BY ran_at, id",
            schedule_id,
        )
        .fetch_all(db_pool)
        .await
        .expect("the read should run")
        .into_iter()
        .map(|run_row| (run_row.how, run_row.moved))
        .collect()
    }

    #[sqlx::test]
    async fn runs_a_due_occurrence_once_and_moves_on_to_the_next(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let schedule_id = seed_schedule(
            &db_pool,
            &new_schedule(&seeded_card, weekly_settings(seeded_card.to_do_list_id)),
        )
        .await;
        let occurrence_at = instant("2030-01-07T10:00:00Z");

        let first_run = run(
            &db_pool,
            user_id,
            schedule_id,
            occurrence_at,
            RunTiming::OnTime,
            instant("2030-01-07T10:00:30Z"),
        )
        .await;
        let retried_run = run(
            &db_pool,
            user_id,
            schedule_id,
            occurrence_at,
            RunTiming::OnTime,
            instant("2030-01-07T10:01:00Z"),
        )
        .await;

        assert_eq!(first_run.next_run_at, Some(instant("2030-01-14T10:00:00Z")));
        assert_eq!(
            retried_run.next_run_at,
            Some(instant("2030-01-14T10:00:00Z"))
        );
        assert_eq!(
            run_rows(&db_pool, schedule_id).await,
            vec![(String::from("on_time"), true)]
        );
    }

    #[sqlx::test]
    async fn ends_a_once_schedule_and_leaves_the_next_run_alone_by_hand(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let once_settings = ScheduleSettings {
            timing: ScheduleTiming {
                repeat_rule: None,
                ..weekly_settings(seeded_card.to_do_list_id).timing
            },
            ..weekly_settings(seeded_card.to_do_list_id)
        };
        let schedule_id = seed_schedule(&db_pool, &new_schedule(&seeded_card, once_settings)).await;
        let occurrence_at = instant("2030-01-07T10:00:00Z");

        let by_hand_run = run(
            &db_pool,
            user_id,
            schedule_id,
            instant("2030-01-01T09:00:00Z"),
            RunTiming::ByHand,
            instant("2030-01-01T09:00:00Z"),
        )
        .await;
        let scheduled_run = run(
            &db_pool,
            user_id,
            schedule_id,
            occurrence_at,
            RunTiming::Late,
            instant("2030-01-07T11:00:00Z"),
        )
        .await;

        assert_eq!(by_hand_run.next_run_at, Some(occurrence_at));
        assert_eq!(scheduled_run.next_run_at, None);
        assert_eq!(
            run_rows(&db_pool, schedule_id).await,
            vec![
                (String::from("by_hand"), true),
                (String::from("late"), false),
            ]
        );
    }
}
