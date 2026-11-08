use sqlx::PgConnection;
use uuid::Uuid;

use crate::features::card_schedules::data_sources::postgres::card_schedule_source::PostgresCardScheduleSource;
use crate::features::card_schedules::entities::card_schedule::CardSchedule;
use crate::features::card_schedules::error::ScheduleError;
use crate::features::cards::actions::lock_board_of_card::LockBoardOfCard;

#[derive(Default)]
pub struct LockSchedule {
    lock_board_of_card: LockBoardOfCard,
    postgres_card_schedule_source: PostgresCardScheduleSource,
}

impl LockSchedule {
    pub fn new() -> Self {
        Self {
            lock_board_of_card: LockBoardOfCard::new(),
            postgres_card_schedule_source: PostgresCardScheduleSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        schedule_id: Uuid,
    ) -> Result<CardSchedule, ScheduleError> {
        let card_id = self
            .postgres_card_schedule_source
            .find_card_id(&mut *connection, user_id, schedule_id)
            .await?
            .ok_or(ScheduleError::NotFound)?;
        self.lock_board_of_card
            .execute(&mut *connection, user_id, card_id)
            .await?;
        self.postgres_card_schedule_source
            .find_locked(&mut *connection, user_id, schedule_id)
            .await?
            .ok_or(ScheduleError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::card_schedules::data_sources::postgres::card_schedule_source::tests::{
        new_schedule, seed_card, seed_schedule, seed_user_id, weekly_settings,
    };

    #[sqlx::test]
    async fn locks_only_the_owner_s_schedule(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let schedule_id = seed_schedule(
            &db_pool,
            &new_schedule(&seeded_card, weekly_settings(seeded_card.to_do_list_id)),
        )
        .await;
        let mut lock_transaction = db_pool.begin().await.expect("a transaction");

        let owner_lock = LockSchedule::new()
            .execute(&mut lock_transaction, user_id, schedule_id)
            .await
            .expect("the owner should lock the schedule");
        let other_lock = LockSchedule::new()
            .execute(&mut lock_transaction, other_user_id, schedule_id)
            .await;

        assert_eq!(owner_lock.id, schedule_id);
        assert!(matches!(other_lock, Err(ScheduleError::NotFound)));
    }
}
