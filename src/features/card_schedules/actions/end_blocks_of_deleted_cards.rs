use sqlx::PgConnection;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::card_schedules::actions::end_schedule_blocks::EndScheduleBlocks;
use crate::features::card_schedules::data_sources::postgres::card_schedule_source::PostgresCardScheduleSource;
use crate::features::card_schedules::entities::card_schedule::DeletedCards;
use crate::features::card_schedules::error::ScheduleError;

#[derive(Default)]
pub struct EndBlocksOfDeletedCards {
    end_schedule_blocks: EndScheduleBlocks,
    postgres_card_schedule_source: PostgresCardScheduleSource,
}

impl EndBlocksOfDeletedCards {
    pub fn new() -> Self {
        Self {
            end_schedule_blocks: EndScheduleBlocks::new(),
            postgres_card_schedule_source: PostgresCardScheduleSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        deleted_cards: DeletedCards,
        now: UtcInstant,
    ) -> Result<(), ScheduleError> {
        let schedule_ids = self
            .postgres_card_schedule_source
            .list_ids_of_deleted_cards(&mut *connection, user_id, deleted_cards)
            .await?;
        if schedule_ids.is_empty() {
            return Ok(());
        }
        self.end_schedule_blocks
            .execute(&mut *connection, &schedule_ids, now)
            .await
    }
}
