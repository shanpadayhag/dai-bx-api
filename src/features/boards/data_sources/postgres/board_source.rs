use sqlx::PgExecutor;
use uuid::Uuid;

use crate::core::idle_length::IdleLength;
use crate::features::boards::entities::board::{
    BoardCard, BoardList, BoardProperty, BoardPropertyKind, BoardSummary, CardValue,
    CardValueContent, NewBoard, PropertyOption,
};
use crate::features::task_groups::entities::uncheck_cutoffs::UncheckCutoffs;

#[derive(Default)]
pub struct PostgresBoardSource;

impl PostgresBoardSource {
    pub async fn list_for_user(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Vec<BoardSummary>, sqlx::Error> {
        sqlx::query_as!(
            BoardSummary,
            "SELECT id, name FROM boards
             WHERE user_id = $1
             ORDER BY lower(name), created_at, id",
            user_id,
        )
        .fetch_all(executor)
        .await
    }

    pub async fn find_summary(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        board_id: Uuid,
    ) -> Result<Option<BoardSummary>, sqlx::Error> {
        sqlx::query_as!(
            BoardSummary,
            "SELECT id, name FROM boards WHERE id = $1 AND user_id = $2",
            board_id,
            user_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn lock_summary(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        board_id: Uuid,
    ) -> Result<Option<BoardSummary>, sqlx::Error> {
        sqlx::query_as!(
            BoardSummary,
            "SELECT id, name FROM boards WHERE id = $1 AND user_id = $2 FOR UPDATE",
            board_id,
            user_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn insert(
        &self,
        executor: impl PgExecutor<'_>,
        new_board: &NewBoard,
    ) -> Result<BoardSummary, sqlx::Error> {
        sqlx::query_as!(
            BoardSummary,
            "INSERT INTO boards (id, user_id, name) VALUES ($1, $2, $3) RETURNING id, name",
            new_board.id,
            new_board.user_id,
            new_board.name,
        )
        .fetch_one(executor)
        .await
    }

    pub async fn rename(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        board_name: &str,
    ) -> Result<BoardSummary, sqlx::Error> {
        sqlx::query_as!(
            BoardSummary,
            "UPDATE boards SET name = $2, updated_at = now() WHERE id = $1 RETURNING id, name",
            board_id,
            board_name,
        )
        .fetch_one(executor)
        .await
    }

    pub async fn delete(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        board_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let delete_outcome = sqlx::query!(
            "DELETE FROM boards WHERE id = $1 AND user_id = $2",
            board_id,
            user_id,
        )
        .execute(executor)
        .await?;
        Ok(delete_outcome.rows_affected() == 1)
    }

    pub async fn list_lists(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
    ) -> Result<Vec<BoardList>, sqlx::Error> {
        let list_rows = sqlx::query!(
            r#"SELECT id,
                      name,
                      position,
                      (date_part('year', archive_after) * 12
                          + date_part('month', archive_after))::integer AS archive_after_months,
                      date_part('day', archive_after)::integer AS archive_after_days
               FROM board_lists
               WHERE board_id = $1
               ORDER BY position"#,
            board_id,
        )
        .fetch_all(executor)
        .await?;
        Ok(list_rows
            .into_iter()
            .map(|list_row| BoardList {
                id: list_row.id,
                name: list_row.name,
                position: list_row.position,
                archive_after: list_row
                    .archive_after_months
                    .zip(list_row.archive_after_days)
                    .map(|(months, days)| IdleLength { months, days }),
            })
            .collect())
    }

    pub async fn list_cards(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        uncheck_cutoffs: &UncheckCutoffs,
    ) -> Result<Vec<BoardCard>, sqlx::Error> {
        let card_rows = sqlx::query!(
            r#"SELECT cards.id,
                      cards.list_id AS "list_id!",
                      cards.position AS "position!",
                      cards.title,
                      to_char(cards.due_date, 'YYYY-MM-DD') AS due_date,
                      to_char(cards.due_time, 'HH24:MI') AS due_time,
                      coalesce(card_task_counts.linked_task_count, 0) AS "linked_task_count!",
                      coalesce(card_task_counts.done_linked_task_count, 0) AS "done_linked_task_count!"
               FROM cards
               LEFT JOIN (
                   SELECT tasks.card_id,
                          count(*) AS linked_task_count,
                          count(*) FILTER (
                              WHERE (tasks.completed_at
                                     >= coalesce(to_timestamp(uncheck_cutoffs.cutoff_millis / 1000.0),
                                                 '-infinity')) IS TRUE
                          ) AS done_linked_task_count
                   FROM tasks
                   JOIN cards AS linked_cards ON linked_cards.id = tasks.card_id
                   LEFT JOIN unnest($2::uuid[], $3::bigint[])
                        AS uncheck_cutoffs(group_id, cutoff_millis)
                        ON uncheck_cutoffs.group_id = tasks.group_id
                   WHERE linked_cards.board_id = $1
                   GROUP BY tasks.card_id
               ) AS card_task_counts ON card_task_counts.card_id = cards.id
               WHERE cards.board_id = $1 AND cards.archived_at IS NULL
               ORDER BY cards.list_id, cards.position"#,
            board_id,
            &uncheck_cutoffs.group_ids,
            &uncheck_cutoffs.cutoff_millis,
        )
        .fetch_all(executor)
        .await?;
        Ok(card_rows
            .into_iter()
            .map(|card_row| BoardCard {
                id: card_row.id,
                list_id: Some(card_row.list_id),
                position: Some(card_row.position),
                title: card_row.title,
                due_date: card_row.due_date,
                due_time: card_row.due_time,
                values: Vec::new(),
                linked_task_count: card_row.linked_task_count,
                done_linked_task_count: card_row.done_linked_task_count,
                next_move: None,
            })
            .collect())
    }

    pub async fn list_card_values(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
    ) -> Result<Vec<(Uuid, CardValue)>, sqlx::Error> {
        let value_rows = sqlx::query!(
            r#"SELECT card_property_values.card_id,
                      card_property_values.property_id,
                      card_property_values.option_id,
                      card_property_values.text_value,
                      card_property_values.number_value::text AS number_value
               FROM card_property_values
               JOIN board_properties ON board_properties.id = card_property_values.property_id
               WHERE card_property_values.board_id = $1
               ORDER BY card_property_values.card_id, board_properties.position"#,
            board_id,
        )
        .fetch_all(executor)
        .await?;
        value_rows
            .into_iter()
            .map(|value_row| {
                let content = CardValueContent::from_columns(
                    value_row.option_id,
                    value_row.text_value,
                    value_row.number_value,
                )
                .ok_or_else(|| sqlx::Error::Decode("a card value has no content".into()))?;
                Ok((
                    value_row.card_id,
                    CardValue {
                        property_id: value_row.property_id,
                        content,
                    },
                ))
            })
            .collect()
    }

    pub async fn list_properties(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
    ) -> Result<Vec<BoardProperty>, sqlx::Error> {
        let property_rows = sqlx::query!(
            r#"SELECT id, name::text AS "name!", kind AS "kind: BoardPropertyKind", position
               FROM board_properties
               WHERE board_id = $1
               ORDER BY position"#,
            board_id,
        )
        .fetch_all(executor)
        .await?;
        Ok(property_rows
            .into_iter()
            .map(|property_row| BoardProperty {
                id: property_row.id,
                name: property_row.name,
                kind: property_row.kind,
                position: property_row.position,
                options: Vec::new(),
            })
            .collect())
    }

    pub async fn list_property_options(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
    ) -> Result<Vec<(Uuid, PropertyOption)>, sqlx::Error> {
        let option_rows = sqlx::query!(
            r#"SELECT property_options.property_id,
                      property_options.id,
                      property_options.name::text AS "name!",
                      property_options.color,
                      property_options.position
               FROM property_options
               JOIN board_properties ON board_properties.id = property_options.property_id
               WHERE board_properties.board_id = $1
               ORDER BY property_options.property_id, property_options.position"#,
            board_id,
        )
        .fetch_all(executor)
        .await?;
        Ok(option_rows
            .into_iter()
            .map(|option_row| {
                (
                    option_row.property_id,
                    PropertyOption {
                        id: option_row.id,
                        name: option_row.name,
                        color: option_row.color,
                        position: option_row.position,
                    },
                )
            })
            .collect())
    }
}

#[cfg(test)]
pub mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::users::actions::create_user::CreateUser;
    use crate::features::users::entities::user::NewUser;

    pub async fn seed_user_id(db_pool: &PgPool, email: &str) -> Uuid {
        CreateUser::new()
            .execute(
                db_pool,
                &NewUser {
                    name: String::from("Shan Padayhag"),
                    email: email.to_owned(),
                    password_hash: String::from("unused"),
                    terms_version: String::from("test-terms-1"),
                },
            )
            .await
            .expect("creating should run")
            .expect("the email should be free")
            .id
    }

    pub async fn seed_board(db_pool: &PgPool, user_id: Uuid, board_name: &str) -> BoardSummary {
        PostgresBoardSource
            .insert(
                db_pool,
                &NewBoard {
                    id: Uuid::now_v7(),
                    user_id,
                    name: board_name.to_owned(),
                },
            )
            .await
            .expect("insert should run")
    }

    #[sqlx::test]
    async fn lists_boards_by_name_ignoring_case_then_oldest_first(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_person_id = seed_user_id(&db_pool, "other@example.com").await;
        seed_board(&db_pool, owner_id, "studies").await;
        let first_personal_board = seed_board(&db_pool, owner_id, "Personal").await;
        seed_board(&db_pool, owner_id, "avvoka").await;
        let second_personal_board = seed_board(&db_pool, owner_id, "personal").await;
        seed_board(&db_pool, other_person_id, "Another").await;

        let listed_boards = PostgresBoardSource
            .list_for_user(&db_pool, owner_id)
            .await
            .expect("listing should run");

        assert_eq!(
            listed_boards
                .iter()
                .map(|board_summary| board_summary.name.as_str())
                .collect::<Vec<_>>(),
            vec!["avvoka", "Personal", "personal", "studies"]
        );
        assert_eq!(listed_boards[1].id, first_personal_board.id);
        assert_eq!(listed_boards[2].id, second_personal_board.id);
    }

    #[sqlx::test]
    async fn finds_and_deletes_only_the_owner_s_board(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_person_id = seed_user_id(&db_pool, "other@example.com").await;
        let owner_board = seed_board(&db_pool, owner_id, "Mine").await;

        let foreign_find = PostgresBoardSource
            .find_summary(&db_pool, other_person_id, owner_board.id)
            .await
            .expect("find should run");
        let foreign_delete = PostgresBoardSource
            .delete(&db_pool, other_person_id, owner_board.id)
            .await
            .expect("delete should run");
        let owner_delete = PostgresBoardSource
            .delete(&db_pool, owner_id, owner_board.id)
            .await
            .expect("delete should run");

        assert_eq!(foreign_find, None);
        assert!(!foreign_delete);
        assert!(owner_delete);
    }

    #[sqlx::test]
    async fn renames_a_board(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let owner_board = seed_board(&db_pool, owner_id, "Personal").await;

        let renamed_board = PostgresBoardSource
            .rename(&db_pool, owner_board.id, "Home")
            .await
            .expect("rename should run");

        assert_eq!(renamed_board.name, "Home");
        assert_eq!(renamed_board.id, owner_board.id);
    }
}
