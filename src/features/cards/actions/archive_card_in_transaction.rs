use sqlx::PgConnection;
use uuid::Uuid;

use crate::features::card_schedules::actions::pause_schedules_of_archived_card::PauseSchedulesOfArchivedCard;
use crate::features::cards::data_sources::postgres::card_move_source::PostgresCardMoveSource;
use crate::features::cards::data_sources::postgres::card_source::PostgresCardSource;
use crate::features::cards::entities::card_archive::CardArchive;
use crate::features::cards::entities::card_move::{MoveCause, NewCardMove};
use crate::features::cards::error::CardError;

#[derive(Default)]
pub struct ArchiveCardInTransaction {
    pause_schedules_of_archived_card: PauseSchedulesOfArchivedCard,
    postgres_card_source: PostgresCardSource,
    postgres_card_move_source: PostgresCardMoveSource,
}

impl ArchiveCardInTransaction {
    pub fn new() -> Self {
        Self {
            pause_schedules_of_archived_card: PauseSchedulesOfArchivedCard::new(),
            postgres_card_source: PostgresCardSource,
            postgres_card_move_source: PostgresCardMoveSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        board_id: Uuid,
        card_id: Uuid,
        card_archive: CardArchive,
    ) -> Result<(), CardError> {
        let current_place = self
            .postgres_card_source
            .find_place(&mut *connection, board_id, card_id)
            .await?
            .ok_or_else(|| CardError::Conflict(String::from("This card is already archived.")))?;
        self.postgres_card_move_source
            .record(
                &mut *connection,
                &NewCardMove {
                    card_id,
                    from_list_id: Some(current_place.list_id),
                    to_list_id: None,
                    cause: &MoveCause::without_task(
                        card_archive.how.move_reason(),
                        card_archive.archived_at,
                    ),
                },
            )
            .await?;
        self.postgres_card_source
            .take_off_board(&mut *connection, card_id, current_place, card_archive)
            .await?;
        self.postgres_card_source
            .close_position_gap(&mut *connection, current_place)
            .await?;
        self.pause_schedules_of_archived_card
            .execute(&mut *connection, card_id, card_archive.archived_at)
            .await?;
        Ok(())
    }
}
