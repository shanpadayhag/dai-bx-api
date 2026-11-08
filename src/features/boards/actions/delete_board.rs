use sqlx::PgPool;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::boards::actions::lock_board::LockBoard;
use crate::features::boards::data_sources::postgres::board_source::PostgresBoardSource;
use crate::features::boards::error::BoardError;
use crate::features::calendar_events::actions::close_running_events::CloseRunningEvents;
use crate::features::calendar_events::entities::calendar_event::ClosingScope;
use crate::features::card_schedules::actions::end_blocks_of_deleted_cards::EndBlocksOfDeletedCards;
use crate::features::card_schedules::entities::card_schedule::DeletedCards;

pub struct DeleteBoard<'a> {
    db_pool: &'a PgPool,
    lock_board: LockBoard,
    close_running_events: CloseRunningEvents,
    end_blocks_of_deleted_cards: EndBlocksOfDeletedCards,
    postgres_board_source: PostgresBoardSource,
}

impl<'a> DeleteBoard<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board: LockBoard::new(),
            close_running_events: CloseRunningEvents::new(),
            end_blocks_of_deleted_cards: EndBlocksOfDeletedCards::new(),
            postgres_board_source: PostgresBoardSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        board_id: Uuid,
        now: UtcInstant,
    ) -> Result<(), BoardError> {
        let mut delete_transaction = self.db_pool.begin().await?;
        self.lock_board
            .execute(&mut *delete_transaction, user_id, board_id)
            .await?;
        self.close_running_events
            .execute(
                &mut delete_transaction,
                user_id,
                ClosingScope::Board(board_id),
            )
            .await?;
        self.end_blocks_of_deleted_cards
            .execute(
                &mut delete_transaction,
                user_id,
                DeletedCards::Board(board_id),
                now,
            )
            .await?;
        self.postgres_board_source
            .delete(&mut *delete_transaction, user_id, board_id)
            .await?;
        delete_transaction.commit().await?;
        Ok(())
    }
}
