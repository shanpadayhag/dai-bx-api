use std::collections::HashMap;

use sqlx::PgPool;
use uuid::Uuid;

use crate::features::board_lists::data_sources::postgres::board_list_source::PostgresBoardListSource;
use crate::features::board_lists::entities::board_list::ListExport;
use crate::features::board_lists::error::BoardListError;

pub struct ExportBoardList<'a> {
    db_pool: &'a PgPool,
    postgres_board_list_source: PostgresBoardListSource,
}

impl<'a> ExportBoardList<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_board_list_source: PostgresBoardListSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        list_id: Uuid,
    ) -> Result<ListExport, BoardListError> {
        let mut export_connection = self.db_pool.acquire().await?;
        let header = self
            .postgres_board_list_source
            .find_export_header(&mut *export_connection, user_id, list_id)
            .await?
            .ok_or(BoardListError::NotFound)?;
        let mut cards = self
            .postgres_board_list_source
            .list_export_cards(&mut *export_connection, list_id)
            .await?;
        let exported_values = self
            .postgres_board_list_source
            .list_export_values(&mut *export_connection, list_id)
            .await?;
        let mut values_by_card = HashMap::with_capacity(cards.len());
        for (card_id, exported_value) in exported_values {
            values_by_card
                .entry(card_id)
                .or_insert_with(Vec::new)
                .push(exported_value);
        }
        for card in &mut cards {
            card.values = values_by_card.remove(&card.id).unwrap_or_default();
        }
        Ok(ListExport { header, cards })
    }
}
