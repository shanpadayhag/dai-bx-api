use sqlx::PgPool;
use uuid::Uuid;

use crate::features::cards::data_sources::postgres::card_move_source::PostgresCardMoveSource;
use crate::features::cards::data_sources::postgres::card_source::PostgresCardSource;
use crate::features::cards::entities::card_move::{CardMovesPage, MovePage};
use crate::features::cards::error::CardError;

pub struct FindCardHistory<'a> {
    db_pool: &'a PgPool,
    postgres_card_source: PostgresCardSource,
    postgres_card_move_source: PostgresCardMoveSource,
}

impl<'a> FindCardHistory<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_card_source: PostgresCardSource,
            postgres_card_move_source: PostgresCardMoveSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        card_id: Uuid,
        move_page: MovePage,
    ) -> Result<CardMovesPage, CardError> {
        let mut history_connection = self.db_pool.acquire().await?;
        self.postgres_card_source
            .find_board_id(&mut *history_connection, user_id, card_id)
            .await?
            .ok_or(CardError::NotFound)?;
        let mut card_moves = self
            .postgres_card_move_source
            .list_for_card(
                &mut *history_connection,
                card_id,
                move_page.before_move_id,
                move_page.limit + 1,
            )
            .await?;
        let has_more = card_moves.len() as i64 > move_page.limit;
        card_moves.truncate(move_page.limit as usize);
        Ok(CardMovesPage {
            moves: card_moves,
            has_more,
        })
    }
}
