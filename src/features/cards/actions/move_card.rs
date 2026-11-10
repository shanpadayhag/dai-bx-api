use sqlx::PgPool;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::boards::actions::build_board::BuildBoard;
use crate::features::boards::entities::board::Board;
use crate::features::cards::actions::lock_board_of_card::LockBoardOfCard;
use crate::features::cards::data_sources::postgres::card_move_source::PostgresCardMoveSource;
use crate::features::cards::data_sources::postgres::card_source::PostgresCardSource;
use crate::features::cards::entities::card::CardPlace;
use crate::features::cards::entities::card_move::{CardMoveReason, MoveCause, NewCardMove};
use crate::features::cards::error::CardError;

pub struct MoveCard<'a> {
    db_pool: &'a PgPool,
    lock_board_of_card: LockBoardOfCard,
    build_board: BuildBoard,
    postgres_card_source: PostgresCardSource,
    postgres_card_move_source: PostgresCardMoveSource,
}

impl<'a> MoveCard<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            lock_board_of_card: LockBoardOfCard::new(),
            build_board: BuildBoard::new(),
            postgres_card_source: PostgresCardSource,
            postgres_card_move_source: PostgresCardMoveSource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        card_id: Uuid,
        requested_place: CardPlace,
        now: UtcInstant,
    ) -> Result<Board, CardError> {
        let mut move_transaction = self.db_pool.begin().await?;
        let board_summary = self
            .lock_board_of_card
            .execute(&mut move_transaction, user_id, card_id)
            .await?;
        let current_place = self
            .postgres_card_source
            .find_place(&mut *move_transaction, board_summary.id, card_id)
            .await?
            .ok_or(CardError::NotFound)?;
        let target_card_count = self
            .postgres_card_source
            .count_in_list(
                &mut *move_transaction,
                board_summary.id,
                requested_place.list_id,
            )
            .await?
            .ok_or_else(|| {
                CardError::Invalid(String::from("a card moves only within its board"))
            })?;
        let is_moved = if requested_place.list_id == current_place.list_id {
            let target_position = requested_place.position.min(target_card_count - 1);
            let is_reordered = target_position != current_place.position;
            if is_reordered {
                self.postgres_card_source
                    .move_within_list(
                        &mut *move_transaction,
                        card_id,
                        current_place,
                        target_position,
                    )
                    .await?;
            }
            is_reordered
        } else {
            let target_place = CardPlace {
                list_id: requested_place.list_id,
                position: requested_place.position.min(target_card_count),
            };
            self.postgres_card_source
                .move_to_other_list(&mut *move_transaction, card_id, current_place, target_place)
                .await?;
            true
        };
        if is_moved {
            self.postgres_card_move_source
                .record(
                    &mut *move_transaction,
                    &NewCardMove {
                        card_id,
                        from_list_id: Some(current_place.list_id),
                        to_list_id: Some(requested_place.list_id),
                        cause: &MoveCause::without_task(CardMoveReason::ByHand, now),
                    },
                )
                .await?;
        }
        let board = self
            .build_board
            .execute(&mut move_transaction, board_summary)
            .await?;
        move_transaction.commit().await?;
        Ok(board)
    }
}
