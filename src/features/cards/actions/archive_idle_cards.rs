use sqlx::PgPool;

use crate::core::utc_instant::UtcInstant;
use crate::features::cards::actions::archive_card_in_transaction::ArchiveCardInTransaction;
use crate::features::cards::actions::lock_board_of_card::LockBoardOfCard;
use crate::features::cards::data_sources::postgres::card_source::PostgresCardSource;
use crate::features::cards::entities::card_archive::{
    ArchiveHow, CardArchive, IDLE_CARDS_PER_CALL, IdleArchiveTally, IdleCard,
};
use crate::features::cards::error::CardError;

pub struct ArchiveIdleCards<'a> {
    db_pool: &'a PgPool,
    lock_board_of_card: LockBoardOfCard,
    archive_card_in_transaction: ArchiveCardInTransaction,
    postgres_card_source: PostgresCardSource,
}

impl<'a> ArchiveIdleCards<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board_of_card: LockBoardOfCard::new(),
            archive_card_in_transaction: ArchiveCardInTransaction::new(),
            postgres_card_source: PostgresCardSource,
        }
    }

    pub async fn execute(&self, now: UtcInstant) -> Result<IdleArchiveTally, CardError> {
        let idle_cards = self
            .postgres_card_source
            .list_idle(self.db_pool, now, IDLE_CARDS_PER_CALL)
            .await?;
        let mut idle_archive_tally = IdleArchiveTally::default();
        for idle_card in idle_cards {
            match self.archive_idle_card(idle_card, now).await {
                Ok(true) => idle_archive_tally.archived_count += 1,
                Ok(false) => {}
                Err(archive_error) => {
                    idle_archive_tally.failed_count += 1;
                    tracing::error!(
                        card_id = %idle_card.card_id,
                        error = %archive_error,
                        "an idle card failed to archive"
                    );
                }
            }
        }
        Ok(idle_archive_tally)
    }

    async fn archive_idle_card(
        &self,
        idle_card: IdleCard,
        now: UtcInstant,
    ) -> Result<bool, CardError> {
        let mut archive_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_card
            .execute(
                &mut archive_transaction,
                idle_card.user_id,
                idle_card.card_id,
            )
            .await?;
        let is_still_idle = self
            .postgres_card_source
            .is_idle(&mut *archive_transaction, idle_card.card_id, now)
            .await?
            && !self
                .postgres_card_source
                .is_running(&mut *archive_transaction, idle_card.card_id)
                .await?;
        if !is_still_idle {
            return Ok(false);
        }
        self.archive_card_in_transaction
            .execute(
                &mut archive_transaction,
                board_summary.id,
                idle_card.card_id,
                CardArchive {
                    archived_at: now,
                    how: ArchiveHow::Idle,
                },
            )
            .await?;
        archive_transaction.commit().await?;
        Ok(true)
    }
}
