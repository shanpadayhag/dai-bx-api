use sqlx::PgExecutor;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::cards::entities::card_move::{CardMove, CardMoveReason, NewCardMove};

#[derive(Default)]
pub struct PostgresCardMoveSource;

impl PostgresCardMoveSource {
    pub async fn record(
        &self,
        executor: impl PgExecutor<'_>,
        new_card_move: &NewCardMove<'_>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "WITH moved_card AS (
                 UPDATE cards
                 SET last_moved_at = 'epoch'::timestamptz + $5::bigint * interval '1 millisecond'
                 WHERE id = $2
                 RETURNING id, user_id
             )
             INSERT INTO card_moves
                 (id, user_id, card_id, moved_at, from_list_name, to_list_name, reason, task_title)
             SELECT $1,
                    moved_card.user_id,
                    moved_card.id,
                    'epoch'::timestamptz + $5::bigint * interval '1 millisecond',
                    (SELECT name FROM board_lists WHERE id = $3),
                    (SELECT name FROM board_lists WHERE id = $4),
                    $6,
                    $7
             FROM moved_card",
            Uuid::now_v7(),
            new_card_move.card_id,
            new_card_move.from_list_id,
            new_card_move.to_list_id,
            new_card_move.cause.moved_at.unix_millis(),
            new_card_move.cause.reason.as_str(),
            new_card_move.cause.task_title.as_deref(),
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn list_for_card(
        &self,
        executor: impl PgExecutor<'_>,
        card_id: Uuid,
        before_move_id: Option<Uuid>,
        row_limit: i64,
    ) -> Result<Vec<CardMove>, sqlx::Error> {
        let move_rows = sqlx::query!(
            r#"SELECT id,
                      (extract(epoch FROM moved_at) * 1000)::bigint AS "moved_at_millis!",
                      from_list_name,
                      to_list_name,
                      reason,
                      task_title
               FROM card_moves
               WHERE card_id = $1 AND ($2::uuid IS NULL OR id < $2)
               ORDER BY id DESC
               LIMIT $3"#,
            card_id,
            before_move_id,
            row_limit,
        )
        .fetch_all(executor)
        .await?;
        move_rows
            .into_iter()
            .map(|move_row| {
                let reason = CardMoveReason::from_stored(&move_row.reason)
                    .ok_or_else(|| sqlx::Error::Decode("an unknown move reason".into()))?;
                Ok(CardMove {
                    id: move_row.id,
                    moved_at: UtcInstant::from_unix_millis(move_row.moved_at_millis),
                    from_list_name: move_row.from_list_name,
                    to_list_name: move_row.to_list_name,
                    reason,
                    task_title: move_row.task_title,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::cards::data_sources::postgres::card_source::tests::{
        seed_board_with_lists, seed_card, seed_user_id,
    };
    use crate::features::cards::entities::card_move::MoveCause;

    #[sqlx::test]
    async fn records_a_move_with_its_column_names_and_sets_the_card_s_clock(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let board = seed_board_with_lists(&db_pool, user_id, &["To Do", "Doing"]).await;
        let card_id = seed_card(&db_pool, user_id, board.board.id, board.lists[0].id, "Plan").await;
        let moved_at = UtcInstant::from_unix_millis(1_893_456_000_000);
        let started_cause = MoveCause {
            reason: CardMoveReason::Started,
            task_title: Some(String::from("Ship")),
            moved_at,
        };
        let by_hand_cause = MoveCause::without_task(CardMoveReason::ByHand, moved_at);

        for (from_list_id, to_list_id, move_cause) in [
            (board.lists[0].id, board.lists[1].id, &started_cause),
            (board.lists[1].id, board.lists[0].id, &by_hand_cause),
        ] {
            PostgresCardMoveSource
                .record(
                    &db_pool,
                    &NewCardMove {
                        card_id,
                        from_list_id: Some(from_list_id),
                        to_list_id: Some(to_list_id),
                        cause: move_cause,
                    },
                )
                .await
                .expect("the move should record");
        }
        let newest_first = PostgresCardMoveSource
            .list_for_card(&db_pool, card_id, None, 50)
            .await
            .expect("the read should run");
        let older_page = PostgresCardMoveSource
            .list_for_card(&db_pool, card_id, Some(newest_first[0].id), 50)
            .await
            .expect("the read should run");
        let last_moved_millis: i64 = sqlx::query_scalar(
            "SELECT (extract(epoch FROM last_moved_at) * 1000)::bigint FROM cards WHERE id = $1",
        )
        .bind(card_id)
        .fetch_one(&db_pool)
        .await
        .expect("the clock should read");

        let shapes: Vec<_> = newest_first
            .iter()
            .map(|card_move| {
                (
                    card_move.from_list_name.as_deref(),
                    card_move.to_list_name.as_deref(),
                    card_move.reason,
                    card_move.task_title.as_deref(),
                    card_move.moved_at,
                )
            })
            .collect();
        assert_eq!(
            shapes,
            vec![
                (
                    Some("Doing"),
                    Some("To Do"),
                    CardMoveReason::ByHand,
                    None,
                    moved_at
                ),
                (
                    Some("To Do"),
                    Some("Doing"),
                    CardMoveReason::Started,
                    Some("Ship"),
                    moved_at
                ),
            ]
        );
        assert_eq!(older_page.len(), 1);
        assert_eq!(older_page[0].reason, CardMoveReason::Started);
        assert_eq!(last_moved_millis, moved_at.unix_millis());
    }
}
