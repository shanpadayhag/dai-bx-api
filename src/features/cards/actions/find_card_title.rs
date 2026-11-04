use sqlx::PgConnection;
use uuid::Uuid;

use crate::features::cards::data_sources::postgres::card_source::PostgresCardSource;
use crate::features::cards::error::CardError;

#[derive(Default)]
pub struct FindCardTitle {
    postgres_card_source: PostgresCardSource,
}

impl FindCardTitle {
    pub fn new() -> Self {
        Self {
            postgres_card_source: PostgresCardSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        user_id: Uuid,
        card_id: Uuid,
    ) -> Result<String, CardError> {
        self.postgres_card_source
            .find_title(&mut *connection, user_id, card_id)
            .await?
            .ok_or(CardError::NotFound)
    }
}
