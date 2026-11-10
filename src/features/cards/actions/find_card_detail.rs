use sqlx::PgPool;
use uuid::Uuid;

use crate::features::card_schedules::actions::list_card_schedules::ListCardSchedules;
use crate::features::card_schedules::entities::card_schedule::next_move_of;
use crate::features::cards::data_sources::postgres::card_source::PostgresCardSource;
use crate::features::cards::data_sources::postgres::card_trigger_source::PostgresCardTriggerSource;
use crate::features::cards::entities::card::CardDetail;
use crate::features::cards::error::CardError;

pub struct FindCardDetail<'a> {
    db_pool: &'a PgPool,
    list_card_schedules: ListCardSchedules,
    postgres_card_source: PostgresCardSource,
    postgres_card_trigger_source: PostgresCardTriggerSource,
}

impl<'a> FindCardDetail<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            list_card_schedules: ListCardSchedules::new(),
            postgres_card_source: PostgresCardSource,
            postgres_card_trigger_source: PostgresCardTriggerSource,
        }
    }

    pub async fn execute(&self, user_id: Uuid, card_id: Uuid) -> Result<CardDetail, CardError> {
        let mut card_connection = self.db_pool.acquire().await?;
        let card_with_notes = self
            .postgres_card_source
            .find_with_notes(&mut *card_connection, user_id, card_id)
            .await?
            .ok_or(CardError::NotFound)?;
        let mut card = card_with_notes.card;
        card.values = self
            .postgres_card_source
            .list_values(&mut *card_connection, card_id)
            .await?;
        let linked_tasks = self
            .postgres_card_source
            .list_linked_tasks(&mut *card_connection, card_id)
            .await?;
        card.linked_task_count = linked_tasks.len() as i64;
        card.done_linked_task_count = linked_tasks
            .iter()
            .filter(|linked_task| linked_task.is_done)
            .count() as i64;
        let schedules = self
            .list_card_schedules
            .execute(&mut card_connection, card_id)
            .await?;
        card.next_move = next_move_of(&schedules);
        let triggers = self
            .postgres_card_trigger_source
            .list_for_card(&mut *card_connection, card_id)
            .await?;
        Ok(CardDetail {
            card,
            notes: card_with_notes.notes,
            linked_tasks,
            tracked_event_count: card_with_notes.tracked_event_count,
            schedules,
            triggers,
            archive: card_with_notes.archive,
        })
    }
}
