use sqlx::PgPool;
use uuid::Uuid;

use crate::features::cards::actions::lock_board_of_card::LockBoardOfCard;
use crate::features::cards::data_sources::postgres::card_trigger_source::PostgresCardTriggerSource;
use crate::features::cards::entities::card_trigger::{CardTriggers, TriggerChoice};
use crate::features::cards::error::CardError;

pub struct SaveCardTriggers<'a> {
    db_pool: &'a PgPool,
    lock_board_of_card: LockBoardOfCard,
    postgres_card_trigger_source: PostgresCardTriggerSource,
}

impl<'a> SaveCardTriggers<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board_of_card: LockBoardOfCard::new(),
            postgres_card_trigger_source: PostgresCardTriggerSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        card_id: Uuid,
        trigger_choice: TriggerChoice,
    ) -> Result<CardTriggers, CardError> {
        let mut save_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_card
            .execute(&mut save_transaction, user_id, card_id)
            .await?;
        let mut chosen_list_ids: Vec<Uuid> = [
            trigger_choice.started_list_id,
            trigger_choice.finished_list_id,
        ]
        .into_iter()
        .flatten()
        .collect();
        chosen_list_ids.dedup();
        let found_list_count = self
            .postgres_card_trigger_source
            .count_lists_on_board(&mut *save_transaction, board_summary.id, &chosen_list_ids)
            .await?;
        if found_list_count != chosen_list_ids.len() as i64 {
            return Err(CardError::NotFound);
        }
        self.postgres_card_trigger_source
            .replace(
                &mut *save_transaction,
                board_summary.id,
                card_id,
                trigger_choice,
            )
            .await?;
        let card_triggers = self
            .postgres_card_trigger_source
            .list_for_card(&mut *save_transaction, card_id)
            .await?;
        save_transaction.commit().await?;
        Ok(card_triggers)
    }
}
