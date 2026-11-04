use sqlx::PgPool;
use uuid::Uuid;

use crate::features::boards::actions::lock_board::LockBoard;
use crate::features::boards::data_sources::postgres::board_source::PostgresBoardSource;
use crate::features::boards::error::BoardError;
use crate::features::calendar_events::actions::close_running_events::CloseRunningEvents;
use crate::features::calendar_events::entities::calendar_event::ClosingScope;

pub struct DeleteBoard<'a> {
    db_pool: &'a PgPool,
    lock_board: LockBoard,
    close_running_events: CloseRunningEvents,
    postgres_board_source: PostgresBoardSource,
}

impl<'a> DeleteBoard<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board: LockBoard::new(),
            close_running_events: CloseRunningEvents::new(),
            postgres_board_source: PostgresBoardSource,
        }
    }

    pub async fn execute(&self, user_id: Uuid, board_id: Uuid) -> Result<(), BoardError> {
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
        self.postgres_board_source
            .delete(&mut *delete_transaction, user_id, board_id)
            .await?;
        delete_transaction.commit().await?;
        Ok(())
    }
}
