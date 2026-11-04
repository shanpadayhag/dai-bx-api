use sqlx::PgPool;
use uuid::Uuid;

use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::entities::board::Board;
use crate::features::calendar_events::actions::rename_tracked_events::RenameTrackedEvents;
use crate::features::calendar_events::entities::calendar_event::TrackedSubject;
use crate::features::cards::actions::lock_board_of_card::LockBoardOfCard;
use crate::features::cards::data_sources::postgres::card_source::PostgresCardSource;
use crate::features::cards::entities::card::CardChanges;
use crate::features::cards::error::CardError;

pub struct UpdateCard<'a> {
    db_pool: &'a PgPool,
    lock_board_of_card: LockBoardOfCard,
    build_board: BuildBoard,
    rename_tracked_events: RenameTrackedEvents,
    postgres_card_source: PostgresCardSource,
}

impl<'a> UpdateCard<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board_of_card: LockBoardOfCard::new(),
            build_board: BuildBoard::new(),
            rename_tracked_events: RenameTrackedEvents::new(),
            postgres_card_source: PostgresCardSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        card_id: Uuid,
        card_changes: &CardChanges,
        renames_events: bool,
    ) -> Result<Board, CardError> {
        let mut update_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_card
            .execute(&mut update_transaction, user_id, card_id)
            .await?;
        let current_due = self
            .postgres_card_source
            .find_due(&mut *update_transaction, board_summary.id, card_id)
            .await?
            .ok_or(CardError::NotFound)?;
        let changed_due = card_changes.apply_to_due(current_due)?;
        self.postgres_card_source
            .update(
                &mut *update_transaction,
                board_summary.id,
                card_id,
                card_changes,
                &changed_due,
            )
            .await?;
        if let Some(card_title) = &card_changes.title
            && renames_events
        {
            self.rename_tracked_events
                .execute(
                    &mut update_transaction,
                    user_id,
                    TrackedSubject::Card(card_id),
                    card_title,
                )
                .await?;
        }
        let board = self
            .build_board
            .execute(&mut update_transaction, board_summary)
            .await?;
        update_transaction.commit().await?;
        Ok(board)
    }
}
