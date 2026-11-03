use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::board_properties::data_sources::postgres::name_save::name_save_outcome;
use crate::features::board_properties::entities::board_property::{
    NameSave, NewBoardProperty, PropertyPlace,
};

#[derive(Default)]
pub struct PostgresBoardPropertySource;

impl PostgresBoardPropertySource {
    pub async fn find_board_id(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        property_id: Uuid,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar!(
            "SELECT board_id FROM board_properties WHERE id = $1 AND user_id = $2",
            property_id,
            user_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn count_on_board(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
    ) -> Result<i32, sqlx::Error> {
        sqlx::query_scalar!(
            r#"SELECT count(*)::integer AS "property_count!" FROM board_properties WHERE board_id = $1"#,
            board_id,
        )
        .fetch_one(executor)
        .await
    }

    pub async fn insert_last(
        &self,
        executor: impl PgExecutor<'_>,
        new_board_property: &NewBoardProperty,
    ) -> Result<NameSave, sqlx::Error> {
        name_save_outcome(
            sqlx::query!(
                "INSERT INTO board_properties (id, user_id, board_id, position, name, kind)
                 SELECT $1, $2, $3, coalesce(max(position) + 1, 0), $4::text::citext, $5
                 FROM board_properties
                 WHERE board_id = $3",
                new_board_property.id,
                new_board_property.user_id,
                new_board_property.board_id,
                new_board_property.name,
                new_board_property.kind as _,
            )
            .execute(executor)
            .await,
        )
    }

    pub async fn rename(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        property_id: Uuid,
        property_name: &str,
    ) -> Result<NameSave, sqlx::Error> {
        name_save_outcome(
            sqlx::query!(
                "UPDATE board_properties SET name = $3::text::citext, updated_at = now()
                 WHERE id = $2 AND board_id = $1",
                board_id,
                property_id,
                property_name,
            )
            .execute(executor)
            .await,
        )
    }

    pub async fn find_place(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        property_id: Uuid,
    ) -> Result<Option<PropertyPlace>, sqlx::Error> {
        sqlx::query_as!(
            PropertyPlace,
            r#"SELECT position,
                      (SELECT count(*)::integer FROM board_properties WHERE board_id = $1) AS "property_count!"
               FROM board_properties
               WHERE id = $2 AND board_id = $1"#,
            board_id,
            property_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn move_to(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        property_id: Uuid,
        current_position: i32,
        target_position: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE board_properties
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
            property_id,
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
        property_id: Uuid,
    ) -> Result<Option<i32>, sqlx::Error> {
        sqlx::query_scalar!(
            "DELETE FROM board_properties WHERE id = $2 AND board_id = $1 RETURNING position",
            board_id,
            property_id,
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
            "UPDATE board_properties SET position = position - 1 WHERE board_id = $1 AND position > $2",
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
    use crate::features::boards::entities::board::BoardPropertyKind;
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

    pub async fn seed_property(
        db_pool: &PgPool,
        user_id: Uuid,
        board_id: Uuid,
        property_name: &str,
        kind: BoardPropertyKind,
    ) -> Uuid {
        let property_id = Uuid::now_v7();
        let property_save = PostgresBoardPropertySource
            .insert_last(
                db_pool,
                &NewBoardProperty {
                    id: property_id,
                    user_id,
                    board_id,
                    name: property_name.to_owned(),
                    kind,
                },
            )
            .await
            .expect("insert should run");
        assert_eq!(property_save, NameSave::Saved);
        property_id
    }

    async fn property_names_by_position(db_pool: &PgPool, board_id: Uuid) -> Vec<String> {
        let property_rows: Vec<(String, i32)> = sqlx::query_as(
            "SELECT name::text, position FROM board_properties WHERE board_id = $1 ORDER BY position",
        )
        .bind(board_id)
        .fetch_all(db_pool)
        .await
        .expect("properties should be read");
        assert_eq!(
            property_rows
                .iter()
                .map(|(_, position)| *position)
                .collect::<Vec<_>>(),
            (0..property_rows.len() as i32).collect::<Vec<_>>()
        );
        property_rows
            .into_iter()
            .map(|(property_name, _)| property_name)
            .collect()
    }

    #[sqlx::test]
    async fn appends_each_property_and_reports_a_name_taken_in_any_case(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let board_id = seed_board_id(&db_pool, owner_id).await;
        seed_property(
            &db_pool,
            owner_id,
            board_id,
            "Size",
            BoardPropertyKind::Choice,
        )
        .await;
        seed_property(
            &db_pool,
            owner_id,
            board_id,
            "Link",
            BoardPropertyKind::Text,
        )
        .await;

        let taken_save = PostgresBoardPropertySource
            .insert_last(
                &db_pool,
                &NewBoardProperty {
                    id: Uuid::now_v7(),
                    user_id: owner_id,
                    board_id,
                    name: String::from("SIZE"),
                    kind: BoardPropertyKind::Number,
                },
            )
            .await
            .expect("insert should run");

        assert_eq!(taken_save, NameSave::NameTaken);
        assert_eq!(
            property_names_by_position(&db_pool, board_id).await,
            vec!["Size", "Link"]
        );
        assert_eq!(
            PostgresBoardPropertySource
                .count_on_board(&db_pool, board_id)
                .await
                .expect("count should run"),
            2
        );
    }

    #[sqlx::test]
    async fn renames_reporting_a_taken_name_and_a_missing_property(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let board_id = seed_board_id(&db_pool, owner_id).await;
        let size_property_id = seed_property(
            &db_pool,
            owner_id,
            board_id,
            "Size",
            BoardPropertyKind::Choice,
        )
        .await;
        seed_property(
            &db_pool,
            owner_id,
            board_id,
            "Link",
            BoardPropertyKind::Text,
        )
        .await;

        let case_only_save = PostgresBoardPropertySource
            .rename(&db_pool, board_id, size_property_id, "SIZE")
            .await
            .expect("rename should run");
        let taken_save = PostgresBoardPropertySource
            .rename(&db_pool, board_id, size_property_id, "link")
            .await
            .expect("rename should run");
        let missing_save = PostgresBoardPropertySource
            .rename(&db_pool, board_id, Uuid::now_v7(), "Effort")
            .await
            .expect("rename should run");

        assert_eq!(case_only_save, NameSave::Saved);
        assert_eq!(taken_save, NameSave::NameTaken);
        assert_eq!(missing_save, NameSave::Missing);
        assert_eq!(
            property_names_by_position(&db_pool, board_id).await,
            vec!["SIZE", "Link"]
        );
    }

    #[sqlx::test]
    async fn moves_a_property_and_closes_the_gap_a_delete_leaves(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let board_id = seed_board_id(&db_pool, owner_id).await;
        let first_property_id =
            seed_property(&db_pool, owner_id, board_id, "A", BoardPropertyKind::Text).await;
        let second_property_id =
            seed_property(&db_pool, owner_id, board_id, "B", BoardPropertyKind::Text).await;
        seed_property(&db_pool, owner_id, board_id, "C", BoardPropertyKind::Text).await;

        let first_place = PostgresBoardPropertySource
            .find_place(&db_pool, board_id, first_property_id)
            .await
            .expect("find should run");
        PostgresBoardPropertySource
            .move_to(&db_pool, board_id, first_property_id, 0, 2)
            .await
            .expect("move should run");
        let after_move = property_names_by_position(&db_pool, board_id).await;
        let removed_position = PostgresBoardPropertySource
            .delete(&db_pool, board_id, second_property_id)
            .await
            .expect("delete should run")
            .expect("the property should be deleted");
        PostgresBoardPropertySource
            .close_position_gap(&db_pool, board_id, removed_position)
            .await
            .expect("closing the gap should run");

        assert_eq!(
            first_place,
            Some(PropertyPlace {
                position: 0,
                property_count: 3,
            })
        );
        assert_eq!(after_move, vec!["B", "C", "A"]);
        assert_eq!(
            property_names_by_position(&db_pool, board_id).await,
            vec!["C", "A"]
        );
    }

    #[sqlx::test]
    async fn finds_the_board_only_for_the_property_s_owner(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_person_id = seed_user_id(&db_pool, "other@example.com").await;
        let board_id = seed_board_id(&db_pool, owner_id).await;
        let property_id = seed_property(
            &db_pool,
            owner_id,
            board_id,
            "Size",
            BoardPropertyKind::Text,
        )
        .await;

        let owner_board_id = PostgresBoardPropertySource
            .find_board_id(&db_pool, owner_id, property_id)
            .await
            .expect("find should run");
        let foreign_board_id = PostgresBoardPropertySource
            .find_board_id(&db_pool, other_person_id, property_id)
            .await
            .expect("find should run");

        assert_eq!(owner_board_id, Some(board_id));
        assert_eq!(foreign_board_id, None);
    }
}
