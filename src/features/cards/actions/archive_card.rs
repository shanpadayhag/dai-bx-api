use sqlx::PgPool;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::cards::actions::archive_card_in_transaction::ArchiveCardInTransaction;
use crate::features::cards::actions::lock_board_of_card::LockBoardOfCard;
use crate::features::cards::entities::card_archive::{ArchiveHow, CardArchive};
use crate::features::cards::error::CardError;

pub struct ArchiveCard<'a> {
    db_pool: &'a PgPool,
    lock_board_of_card: LockBoardOfCard,
    archive_card_in_transaction: ArchiveCardInTransaction,
}

impl<'a> ArchiveCard<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board_of_card: LockBoardOfCard::new(),
            archive_card_in_transaction: ArchiveCardInTransaction::new(),
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        card_id: Uuid,
        now: UtcInstant,
    ) -> Result<(), CardError> {
        let mut archive_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_card
            .execute(&mut archive_transaction, user_id, card_id)
            .await?;
        self.archive_card_in_transaction
            .execute(
                &mut archive_transaction,
                board_summary.id,
                card_id,
                CardArchive {
                    archived_at: now,
                    how: ArchiveHow::ByHand,
                },
            )
            .await?;
        archive_transaction.commit().await?;
        Ok(())
    }
}
