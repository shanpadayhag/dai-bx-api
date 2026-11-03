use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::board_lists::entities::board_list::{
    ExportedCard, ExportedValue, ListExportHeader, ListPlace, NewBoardList,
};

#[derive(Default)]
pub struct PostgresBoardListSource;

impl PostgresBoardListSource {
    pub async fn find_board_id(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        list_id: Uuid,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar!(
            "SELECT board_id FROM board_lists WHERE id = $1 AND user_id = $2",
            list_id,
            user_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn find_export_header(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        list_id: Uuid,
    ) -> Result<Option<ListExportHeader>, sqlx::Error> {
        sqlx::query_as!(
            ListExportHeader,
            "SELECT boards.name AS board_name, board_lists.name AS list_name
             FROM board_lists
             JOIN boards ON boards.id = board_lists.board_id
             WHERE board_lists.id = $1 AND board_lists.user_id = $2",
            list_id,
            user_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn list_export_cards(
        &self,
        executor: impl PgExecutor<'_>,
        list_id: Uuid,
    ) -> Result<Vec<ExportedCard>, sqlx::Error> {
        let card_rows = sqlx::query!(
            r#"SELECT id,
                      title,
                      to_char(due_date, 'YYYY-MM-DD') AS due_date,
                      to_char(due_time, 'HH24:MI') AS due_time,
                      notes
               FROM cards
               WHERE list_id = $1
               ORDER BY position"#,
            list_id,
        )
        .fetch_all(executor)
        .await?;
        Ok(card_rows
            .into_iter()
            .map(|card_row| ExportedCard {
                id: card_row.id,
                title: card_row.title,
                due_date: card_row.due_date,
                due_time: card_row.due_time,
                values: Vec::new(),
                notes: card_row.notes,
            })
            .collect())
    }

    pub async fn list_export_values(
        &self,
        executor: impl PgExecutor<'_>,
        list_id: Uuid,
    ) -> Result<Vec<(Uuid, ExportedValue)>, sqlx::Error> {
        let value_rows = sqlx::query!(
            r#"SELECT card_property_values.card_id,
                      board_properties.name::text AS "property_name!",
                      coalesce(property_options.name::text,
                               card_property_values.text_value,
                               card_property_values.number_value::text) AS "value!"
               FROM card_property_values
               JOIN cards ON cards.id = card_property_values.card_id
               JOIN board_properties ON board_properties.id = card_property_values.property_id
               LEFT JOIN property_options ON property_options.id = card_property_values.option_id
               WHERE cards.list_id = $1
               ORDER BY card_property_values.card_id, board_properties.position"#,
            list_id,
        )
        .fetch_all(executor)
        .await?;
        Ok(value_rows
            .into_iter()
            .map(|value_row| {
                (
                    value_row.card_id,
                    ExportedValue {
                        property_name: value_row.property_name,
                        value: value_row.value,
                    },
                )
            })
            .collect())
    }

    pub async fn insert_last(
        &self,
        executor: impl PgExecutor<'_>,
        new_board_list: &NewBoardList,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "INSERT INTO board_lists (id, user_id, board_id, position, name)
             SELECT $1, $2, $3, coalesce(max(position) + 1, 0), $4
             FROM board_lists
             WHERE board_id = $3",
            new_board_list.id,
            new_board_list.user_id,
            new_board_list.board_id,
            new_board_list.name,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn rename(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        list_id: Uuid,
        list_name: &str,
    ) -> Result<bool, sqlx::Error> {
        let rename_outcome = sqlx::query!(
            "UPDATE board_lists SET name = $3, updated_at = now() WHERE id = $2 AND board_id = $1",
            board_id,
            list_id,
            list_name,
        )
        .execute(executor)
        .await?;
        Ok(rename_outcome.rows_affected() == 1)
    }

    pub async fn find_place(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        list_id: Uuid,
    ) -> Result<Option<ListPlace>, sqlx::Error> {
        sqlx::query_as!(
            ListPlace,
            r#"SELECT position,
                      (SELECT count(*)::integer FROM board_lists WHERE board_id = $1) AS "list_count!"
               FROM board_lists
               WHERE id = $2 AND board_id = $1"#,
            board_id,
            list_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn move_to(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        list_id: Uuid,
        current_position: i32,
        target_position: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE board_lists
             SET position = CASE
                     WHEN id = $2 THEN $4::integer
                     WHEN $3::integer < $4::integer THEN position - 1
                     ELSE position + 1
                 END,
                 updated_at = CASE WHEN id = $2 THEN now() ELSE updated_at END
             WHERE board_id = $1
               AND (id = $2 OR position BETWEEN least($3::integer, $4::integer)
                                    AND greatest($3::integer, $4::integer))",
            board_id,
            list_id,
            current_position,
            target_position,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn delete(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        list_id: Uuid,
    ) -> Result<Option<i32>, sqlx::Error> {
        sqlx::query_scalar!(
            "DELETE FROM board_lists WHERE id = $2 AND board_id = $1 RETURNING position",
            board_id,
            list_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn close_position_gap(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        removed_position: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE board_lists SET position = position - 1 WHERE board_id = $1 AND position > $2",
            board_id,
            removed_position,
        )
        .execute(executor)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::boards::actions::create_board::CreateBoard;
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

    pub async fn seed_board_id(db_pool: &PgPool, user_id: Uuid) -> Uuid {
        CreateBoard::new()
            .execute(db_pool, user_id, "Personal")
            .await
            .expect("the board should be created")
            .board
            .id
    }

    pub async fn seed_list(
        db_pool: &PgPool,
        user_id: Uuid,
        board_id: Uuid,
        list_name: &str,
    ) -> Uuid {
        let list_id = Uuid::now_v7();
        PostgresBoardListSource
            .insert_last(
                db_pool,
                &NewBoardList {
                    id: list_id,
                    user_id,
                    board_id,
                    name: list_name.to_owned(),
                },
            )
            .await
            .expect("insert should run");
        list_id
    }

    async fn list_names_by_position(db_pool: &PgPool, board_id: Uuid) -> Vec<(String, i32)> {
        sqlx::query_as(
            "SELECT name, position FROM board_lists WHERE board_id = $1 ORDER BY position",
        )
        .bind(board_id)
        .fetch_all(db_pool)
        .await
        .expect("lists should be read")
    }

    fn places(expected_names: &[&str]) -> Vec<(String, i32)> {
        expected_names
            .iter()
            .zip(0..)
            .map(|(list_name, position)| ((*list_name).to_owned(), position))
            .collect()
    }

    #[sqlx::test]
    async fn appends_each_list_after_the_board_s_last(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let board_id = seed_board_id(&db_pool, owner_id).await;
        let other_board_id = seed_board_id(&db_pool, owner_id).await;
        seed_list(&db_pool, owner_id, other_board_id, "Elsewhere").await;

        for list_name in ["A", "B", "C"] {
            seed_list(&db_pool, owner_id, board_id, list_name).await;
        }

        assert_eq!(
            list_names_by_position(&db_pool, board_id).await,
            places(&["A", "B", "C"])
        );
    }

    #[sqlx::test]
    async fn moves_a_list_both_ways_shifting_only_the_lists_between(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let board_id = seed_board_id(&db_pool, owner_id).await;
        let first_list_id = seed_list(&db_pool, owner_id, board_id, "A").await;
        for list_name in ["B", "C", "D"] {
            seed_list(&db_pool, owner_id, board_id, list_name).await;
        }

        PostgresBoardListSource
            .move_to(&db_pool, board_id, first_list_id, 0, 2)
            .await
            .expect("move should run");
        let after_right_move = list_names_by_position(&db_pool, board_id).await;
        PostgresBoardListSource
            .move_to(&db_pool, board_id, first_list_id, 2, 1)
            .await
            .expect("move should run");

        assert_eq!(after_right_move, places(&["B", "C", "A", "D"]));
        assert_eq!(
            list_names_by_position(&db_pool, board_id).await,
            places(&["B", "A", "C", "D"])
        );
    }

    #[sqlx::test]
    async fn finds_a_list_s_place_and_the_board_s_list_count(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let board_id = seed_board_id(&db_pool, owner_id).await;
        seed_list(&db_pool, owner_id, board_id, "A").await;
        let second_list_id = seed_list(&db_pool, owner_id, board_id, "B").await;
        let other_board_id = seed_board_id(&db_pool, owner_id).await;

        let list_place = PostgresBoardListSource
            .find_place(&db_pool, board_id, second_list_id)
            .await
            .expect("find should run");
        let place_on_another_board = PostgresBoardListSource
            .find_place(&db_pool, other_board_id, second_list_id)
            .await
            .expect("find should run");

        assert_eq!(
            list_place,
            Some(ListPlace {
                position: 1,
                list_count: 2,
            })
        );
        assert_eq!(place_on_another_board, None);
    }

    #[sqlx::test]
    async fn deletes_a_list_and_closes_its_gap(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let board_id = seed_board_id(&db_pool, owner_id).await;
        seed_list(&db_pool, owner_id, board_id, "A").await;
        let second_list_id = seed_list(&db_pool, owner_id, board_id, "B").await;
        seed_list(&db_pool, owner_id, board_id, "C").await;

        let removed_position = PostgresBoardListSource
            .delete(&db_pool, board_id, second_list_id)
            .await
            .expect("delete should run")
            .expect("the list should be deleted");
        PostgresBoardListSource
            .close_position_gap(&db_pool, board_id, removed_position)
            .await
            .expect("closing the gap should run");

        assert_eq!(
            list_names_by_position(&db_pool, board_id).await,
            places(&["A", "C"])
        );
    }

    #[sqlx::test]
    async fn finds_the_board_only_for_the_list_s_owner(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_person_id = seed_user_id(&db_pool, "other@example.com").await;
        let board_id = seed_board_id(&db_pool, owner_id).await;
        let list_id = seed_list(&db_pool, owner_id, board_id, "A").await;

        let owner_board_id = PostgresBoardListSource
            .find_board_id(&db_pool, owner_id, list_id)
            .await
            .expect("find should run");
        let foreign_board_id = PostgresBoardListSource
            .find_board_id(&db_pool, other_person_id, list_id)
            .await
            .expect("find should run");

        assert_eq!(owner_board_id, Some(board_id));
        assert_eq!(foreign_board_id, None);
    }
}
