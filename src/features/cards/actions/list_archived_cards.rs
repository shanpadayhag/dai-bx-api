use sqlx::PgPool;
use uuid::Uuid;

use crate::features::cards::data_sources::postgres::card_source::PostgresCardSource;
use crate::features::cards::entities::card_archive::{ARCHIVED_CARD_PAGE_LIMIT, ArchivedCardsPage};
use crate::features::cards::error::CardError;

pub struct ListArchivedCards<'a> {
    db_pool: &'a PgPool,
    postgres_card_source: PostgresCardSource,
}

impl<'a> ListArchivedCards<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_card_source: PostgresCardSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        board_id: Uuid,
        before_card_id: Option<Uuid>,
    ) -> Result<ArchivedCardsPage, CardError> {
        let mut list_connection = self.db_pool.acquire().await?;
        if !self
            .postgres_card_source
            .is_board_owned(&mut *list_connection, user_id, board_id)
            .await?
        {
            return Err(CardError::NotFound);
        }
        let mut archived_cards = self
            .postgres_card_source
            .list_archived(
                &mut *list_connection,
                board_id,
                before_card_id,
                ARCHIVED_CARD_PAGE_LIMIT + 1,
            )
            .await?;
        let has_more = archived_cards.len() as i64 > ARCHIVED_CARD_PAGE_LIMIT;
        archived_cards.truncate(ARCHIVED_CARD_PAGE_LIMIT as usize);
        Ok(ArchivedCardsPage {
            cards: archived_cards,
            has_more,
        })
    }
}
