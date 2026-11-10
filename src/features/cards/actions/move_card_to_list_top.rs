use sqlx::PgConnection;
use uuid::Uuid;

use crate::features::cards::data_sources::postgres::card_move_source::PostgresCardMoveSource;
use crate::features::cards::data_sources::postgres::card_source::PostgresCardSource;
use crate::features::cards::entities::card::CardPlace;
use crate::features::cards::entities::card_move::{MoveCause, NewCardMove};
use crate::features::cards::error::CardError;

#[derive(Default)]
pub struct MoveCardToListTop {
    postgres_card_source: PostgresCardSource,
    postgres_card_move_source: PostgresCardMoveSource,
}

impl MoveCardToListTop {
    pub fn new() -> Self {
        Self {
            postgres_card_source: PostgresCardSource,
            postgres_card_move_source: PostgresCardMoveSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        board_id: Uuid,
        card_id: Uuid,
        target_list_id: Uuid,
        move_cause: &MoveCause,
    ) -> Result<bool, CardError> {
        let Some(current_place) = self
            .postgres_card_source
            .find_place(&mut *connection, board_id, card_id)
            .await?
        else {
            return Ok(false);
        };
        if current_place.list_id == target_list_id {
            return Ok(false);
        }
        self.postgres_card_source
            .count_in_list(&mut *connection, board_id, target_list_id)
            .await?
            .ok_or(CardError::NotFound)?;
        self.postgres_card_source
            .move_to_other_list(
                &mut *connection,
                card_id,
                current_place,
                CardPlace {
                    list_id: target_list_id,
                    position: 0,
                },
            )
            .await?;
        self.postgres_card_move_source
            .record(
                &mut *connection,
                &NewCardMove {
                    card_id,
                    from_list_id: Some(current_place.list_id),
                    to_list_id: Some(target_list_id),
                    cause: move_cause,
                },
            )
            .await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::core::utc_instant::UtcInstant;
    use crate::features::cards::data_sources::postgres::card_source::tests::{
        seed_board_with_lists, seed_card, seed_user_id,
    };
    use crate::features::cards::entities::card_move::CardMoveReason;

    async fn place_of(db_pool: &PgPool, board_id: Uuid, card_id: Uuid) -> CardPlace {
        PostgresCardSource
            .find_place(db_pool, board_id, card_id)
            .await
            .expect("the read should run")
            .expect("the card should exist")
    }

    #[sqlx::test]
    async fn moves_a_card_to_the_top_of_another_list_only(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let board = seed_board_with_lists(&db_pool, user_id, &["To Do", "Doing"]).await;
        let (to_do_list_id, doing_list_id) = (board.lists[0].id, board.lists[1].id);
        let board_id = board.board.id;
        let waiting_card_id = seed_card(&db_pool, user_id, board_id, to_do_list_id, "Plan").await;
        let moving_card_id =
            seed_card(&db_pool, user_id, board_id, doing_list_id, "Weekly review").await;
        let mut move_transaction = db_pool.begin().await.expect("a transaction");

        let first_move = MoveCardToListTop::new()
            .execute(
                &mut move_transaction,
                board_id,
                moving_card_id,
                to_do_list_id,
                &MoveCause::without_task(CardMoveReason::Schedule, UtcInstant::now()),
            )
            .await
            .expect("the move should run");
        let second_move = MoveCardToListTop::new()
            .execute(
                &mut move_transaction,
                board_id,
                moving_card_id,
                to_do_list_id,
                &MoveCause::without_task(CardMoveReason::Schedule, UtcInstant::now()),
            )
            .await
            .expect("the move should run");
        move_transaction
            .commit()
            .await
            .expect("the commit should run");

        assert!(first_move);
        assert!(!second_move);
        assert_eq!(
            place_of(&db_pool, board_id, moving_card_id).await,
            CardPlace {
                list_id: to_do_list_id,
                position: 0,
            }
        );
        assert_eq!(
            place_of(&db_pool, board_id, waiting_card_id).await,
            CardPlace {
                list_id: to_do_list_id,
                position: 1,
            }
        );
    }
}
