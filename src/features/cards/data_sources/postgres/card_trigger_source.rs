use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::cards::entities::card_trigger::{
    CardTriggers, TriggerChoice, TriggerKind, TriggerTarget,
};

#[derive(Default)]
pub struct PostgresCardTriggerSource;

impl PostgresCardTriggerSource {
    pub async fn list_for_card(
        &self,
        executor: impl PgExecutor<'_>,
        card_id: Uuid,
    ) -> Result<CardTriggers, sqlx::Error> {
        let trigger_rows = sqlx::query!(
            r#"SELECT card_triggers.kind AS "kind!",
                      card_triggers.list_id,
                      board_lists.name AS "list_name?"
               FROM card_triggers
               LEFT JOIN board_lists ON board_lists.id = card_triggers.list_id
               WHERE card_triggers.card_id = $1"#,
            card_id,
        )
        .fetch_all(executor)
        .await?;
        let mut card_triggers = CardTriggers::default();
        for trigger_row in trigger_rows {
            let trigger_target = Some(TriggerTarget {
                list_id: trigger_row.list_id,
                list_name: trigger_row.list_name,
            });
            if trigger_row.kind == TriggerKind::Started.as_str() {
                card_triggers.started = trigger_target;
            } else {
                card_triggers.finished = trigger_target;
            }
        }
        Ok(card_triggers)
    }

    pub async fn find_target_list_id(
        &self,
        executor: impl PgExecutor<'_>,
        card_id: Uuid,
        trigger_kind: TriggerKind,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar!(
            r#"SELECT card_triggers.list_id AS "list_id!"
               FROM card_triggers
               JOIN cards ON cards.id = card_triggers.card_id
               WHERE card_triggers.card_id = $1
                 AND card_triggers.kind = $2
                 AND card_triggers.list_id IS NOT NULL
                 AND cards.archived_at IS NULL"#,
            card_id,
            trigger_kind.as_str(),
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn count_lists_on_board(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        list_ids: &[Uuid],
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar!(
            r#"SELECT count(*) AS "list_count!"
               FROM board_lists
               WHERE board_id = $1 AND id = ANY($2)"#,
            board_id,
            list_ids,
        )
        .fetch_one(executor)
        .await
    }

    pub async fn replace(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        card_id: Uuid,
        trigger_choice: TriggerChoice,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "WITH chosen_triggers (kind, list_id) AS (
                 VALUES ('started', $3::uuid), ('finished', $4::uuid)
             ),
             cleared_triggers AS (
                 DELETE FROM card_triggers
                 USING chosen_triggers
                 WHERE card_triggers.card_id = $2
                   AND card_triggers.kind = chosen_triggers.kind
                   AND chosen_triggers.list_id IS NULL
             )
             INSERT INTO card_triggers (card_id, kind, user_id, board_id, list_id)
             SELECT cards.id, chosen_triggers.kind, cards.user_id, cards.board_id,
                    chosen_triggers.list_id
             FROM cards
             CROSS JOIN chosen_triggers
             WHERE cards.id = $2 AND cards.board_id = $1 AND chosen_triggers.list_id IS NOT NULL
             ON CONFLICT (card_id, kind)
             DO UPDATE SET list_id = excluded.list_id, updated_at = now()",
            board_id,
            card_id,
            trigger_choice.started_list_id,
            trigger_choice.finished_list_id,
        )
        .execute(executor)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::cards::data_sources::postgres::card_source::tests::{
        seed_board_with_lists, seed_card, seed_user_id,
    };

    #[sqlx::test]
    async fn replaces_both_one_or_neither_and_flags_a_deleted_column(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let board = seed_board_with_lists(&db_pool, user_id, &["To Do", "Doing", "Done"]).await;
        let (doing_list_id, done_list_id) = (board.lists[1].id, board.lists[2].id);
        let card_id = seed_card(&db_pool, user_id, board.board.id, board.lists[0].id, "Plan").await;
        let source = PostgresCardTriggerSource;
        let mut saved_triggers = Vec::new();

        for trigger_choice in [
            TriggerChoice {
                started_list_id: Some(doing_list_id),
                finished_list_id: Some(done_list_id),
            },
            TriggerChoice {
                started_list_id: Some(done_list_id),
                finished_list_id: None,
            },
        ] {
            source
                .replace(&db_pool, board.board.id, card_id, trigger_choice)
                .await
                .expect("the replace should run");
            saved_triggers.push(
                source
                    .list_for_card(&db_pool, card_id)
                    .await
                    .expect("the read should run"),
            );
        }
        sqlx::query("DELETE FROM board_lists WHERE id = $1")
            .bind(done_list_id)
            .execute(&db_pool)
            .await
            .expect("the column should delete");
        let flagged_triggers = source
            .list_for_card(&db_pool, card_id)
            .await
            .expect("the read should run");
        let flagged_target_id = source
            .find_target_list_id(&db_pool, card_id, TriggerKind::Started)
            .await
            .expect("the read should run");

        let target = |list_id: Uuid, list_name: &str| {
            Some(TriggerTarget {
                list_id: Some(list_id),
                list_name: Some(list_name.to_owned()),
            })
        };
        assert_eq!(
            saved_triggers,
            vec![
                CardTriggers {
                    started: target(doing_list_id, "Doing"),
                    finished: target(done_list_id, "Done"),
                },
                CardTriggers {
                    started: target(done_list_id, "Done"),
                    finished: None,
                },
            ]
        );
        assert_eq!(
            flagged_triggers,
            CardTriggers {
                started: Some(TriggerTarget {
                    list_id: None,
                    list_name: None,
                }),
                finished: None,
            }
        );
        assert_eq!(flagged_target_id, None);
    }
}
