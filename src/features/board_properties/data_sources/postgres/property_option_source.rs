use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::board_properties::data_sources::postgres::name_save::name_save_outcome;
use crate::features::board_properties::entities::board_property::{
    NameSave, NewPropertyOption, OptionChanges, OptionPlace, PropertyOptionCount,
};
use crate::features::boards::entities::board::BoardPropertyKind;

#[derive(Default)]
pub struct PostgresPropertyOptionSource;

impl PostgresPropertyOptionSource {
    pub async fn find_board_id(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        option_id: Uuid,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar!(
            "SELECT board_properties.board_id
             FROM property_options
             JOIN board_properties ON board_properties.id = property_options.property_id
             WHERE property_options.id = $1 AND property_options.user_id = $2",
            option_id,
            user_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn find_option_count(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        property_id: Uuid,
    ) -> Result<Option<PropertyOptionCount>, sqlx::Error> {
        sqlx::query_as!(
            PropertyOptionCount,
            r#"SELECT kind AS "kind: BoardPropertyKind",
                      (SELECT count(*)::integer FROM property_options WHERE property_id = $2) AS "option_count!"
               FROM board_properties
               WHERE id = $2 AND board_id = $1"#,
            board_id,
            property_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn insert_for_new_property(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        property_id: Uuid,
        new_property_options: &[NewPropertyOption],
    ) -> Result<NameSave, sqlx::Error> {
        let option_ids: Vec<Uuid> = new_property_options
            .iter()
            .map(|new_property_option| new_property_option.id)
            .collect();
        let option_names: Vec<String> = new_property_options
            .iter()
            .map(|new_property_option| new_property_option.name.clone())
            .collect();
        let option_colors: Vec<Option<String>> = new_property_options
            .iter()
            .map(|new_property_option| new_property_option.color.clone())
            .collect();
        name_save_outcome(
            sqlx::query!(
                "INSERT INTO property_options (id, user_id, property_id, position, name, color)
                 SELECT new_options.option_id, $1, $2, (new_options.option_number - 1)::integer,
                        new_options.option_name, new_options.option_color
                 FROM unnest($3::uuid[], $4::text[], $5::text[])
                      WITH ORDINALITY AS new_options (option_id, option_name, option_color, option_number)",
                user_id,
                property_id,
                &option_ids,
                &option_names,
                &option_colors as &[Option<String>],
            )
            .execute(executor)
            .await,
        )
    }

    pub async fn insert_last(
        &self,
        executor: impl PgExecutor<'_>,
        new_property_option: &NewPropertyOption,
    ) -> Result<NameSave, sqlx::Error> {
        name_save_outcome(
            sqlx::query!(
                "INSERT INTO property_options (id, user_id, property_id, position, name, color)
                 SELECT $1, $2, $3, coalesce(max(position) + 1, 0), $4::text::citext, $5
                 FROM property_options
                 WHERE property_id = $3",
                new_property_option.id,
                new_property_option.user_id,
                new_property_option.property_id,
                new_property_option.name,
                new_property_option.color,
            )
            .execute(executor)
            .await,
        )
    }

    pub async fn update(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        option_id: Uuid,
        option_changes: &OptionChanges,
    ) -> Result<NameSave, sqlx::Error> {
        name_save_outcome(
            sqlx::query!(
                "UPDATE property_options
                 SET name = coalesce($3::text::citext, property_options.name),
                     color = CASE WHEN $4::boolean THEN $5 ELSE property_options.color END,
                     updated_at = now()
                 FROM board_properties
                 WHERE property_options.id = $2
                   AND board_properties.id = property_options.property_id
                   AND board_properties.board_id = $1",
                board_id,
                option_id,
                option_changes.name,
                option_changes.color_change.is_some(),
                option_changes
                    .color_change
                    .as_ref()
                    .and_then(Option::as_deref),
            )
            .execute(executor)
            .await,
        )
    }

    pub async fn find_place(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        option_id: Uuid,
    ) -> Result<Option<OptionPlace>, sqlx::Error> {
        sqlx::query_as!(
            OptionPlace,
            r#"SELECT property_options.property_id,
                      property_options.position,
                      (SELECT count(*)::integer
                       FROM property_options AS sibling_options
                       WHERE sibling_options.property_id = property_options.property_id) AS "option_count!"
               FROM property_options
               JOIN board_properties ON board_properties.id = property_options.property_id
               WHERE property_options.id = $2 AND board_properties.board_id = $1"#,
            board_id,
            option_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn move_to(
        &self,
        executor: impl PgExecutor<'_>,
        property_id: Uuid,
        option_id: Uuid,
        current_position: i32,
        target_position: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE property_options
             SET position = CASE
                     WHEN id = $2 THEN $4::integer
                     WHEN $3::integer < $4::integer THEN position - 1
                     ELSE position + 1
                 END,
                 updated_at = CASE WHEN id = $2 THEN now() ELSE updated_at END
             WHERE property_id = $1
               AND (id = $2 OR position BETWEEN least($3::integer, $4::integer)
                                    AND greatest($3::integer, $4::integer))",
            property_id,
            option_id,
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
        property_id: Uuid,
        option_id: Uuid,
    ) -> Result<Option<i32>, sqlx::Error> {
        sqlx::query_scalar!(
            "DELETE FROM property_options WHERE id = $2 AND property_id = $1 RETURNING position",
            property_id,
            option_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn close_position_gap(
        &self,
        executor: impl PgExecutor<'_>,
        property_id: Uuid,
        removed_position: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE property_options SET position = position - 1 WHERE property_id = $1 AND position > $2",
            property_id,
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
    use crate::features::board_properties::data_sources::postgres::board_property_source::tests::{
        seed_board_id, seed_property, seed_user_id,
    };

    pub fn new_option(
        user_id: Uuid,
        property_id: Uuid,
        option_name: &str,
        option_color: Option<&str>,
    ) -> NewPropertyOption {
        NewPropertyOption {
            id: Uuid::now_v7(),
            user_id,
            property_id,
            name: option_name.to_owned(),
            color: option_color.map(str::to_owned),
        }
    }

    async fn option_rows(db_pool: &PgPool, property_id: Uuid) -> Vec<(String, Option<String>)> {
        let option_rows: Vec<(String, Option<String>, i32)> = sqlx::query_as(
            "SELECT name::text, color, position FROM property_options
             WHERE property_id = $1 ORDER BY position",
        )
        .bind(property_id)
        .fetch_all(db_pool)
        .await
        .expect("options should be read");
        assert_eq!(
            option_rows
                .iter()
                .map(|(_, _, position)| *position)
                .collect::<Vec<_>>(),
            (0..option_rows.len() as i32).collect::<Vec<_>>()
        );
        option_rows
            .into_iter()
            .map(|(option_name, option_color, _)| (option_name, option_color))
            .collect()
    }

    fn named(option_name: &str, option_color: Option<&str>) -> (String, Option<String>) {
        (option_name.to_owned(), option_color.map(str::to_owned))
    }

    #[sqlx::test]
    async fn inserts_a_new_property_s_options_in_order_then_appends(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let board_id = seed_board_id(&db_pool, owner_id).await;
        let property_id = seed_property(
            &db_pool,
            owner_id,
            board_id,
            "Size",
            BoardPropertyKind::Choice,
        )
        .await;

        let first_save = PostgresPropertyOptionSource
            .insert_for_new_property(
                &db_pool,
                owner_id,
                property_id,
                &[
                    new_option(owner_id, property_id, "Small", Some("#22c55e")),
                    new_option(owner_id, property_id, "Large", None),
                ],
            )
            .await
            .expect("insert should run");
        let appended_save = PostgresPropertyOptionSource
            .insert_last(&db_pool, &new_option(owner_id, property_id, "Huge", None))
            .await
            .expect("insert should run");
        let taken_save = PostgresPropertyOptionSource
            .insert_last(&db_pool, &new_option(owner_id, property_id, "SMALL", None))
            .await
            .expect("insert should run");

        assert_eq!(first_save, NameSave::Saved);
        assert_eq!(appended_save, NameSave::Saved);
        assert_eq!(taken_save, NameSave::NameTaken);
        assert_eq!(
            option_rows(&db_pool, property_id).await,
            vec![
                named("Small", Some("#22c55e")),
                named("Large", None),
                named("Huge", None),
            ]
        );
        assert_eq!(
            PostgresPropertyOptionSource
                .find_option_count(&db_pool, board_id, property_id)
                .await
                .expect("find should run"),
            Some(PropertyOptionCount {
                kind: BoardPropertyKind::Choice,
                option_count: 3,
            })
        );
    }

    #[sqlx::test]
    async fn refuses_duplicate_names_inside_one_new_property(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let board_id = seed_board_id(&db_pool, owner_id).await;
        let property_id = seed_property(
            &db_pool,
            owner_id,
            board_id,
            "Size",
            BoardPropertyKind::Choice,
        )
        .await;

        let taken_save = PostgresPropertyOptionSource
            .insert_for_new_property(
                &db_pool,
                owner_id,
                property_id,
                &[
                    new_option(owner_id, property_id, "Small", None),
                    new_option(owner_id, property_id, "small", None),
                ],
            )
            .await
            .expect("insert should run");

        assert_eq!(taken_save, NameSave::NameTaken);
    }

    #[sqlx::test]
    async fn renames_recolors_and_clears_a_color_independently(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let board_id = seed_board_id(&db_pool, owner_id).await;
        let property_id = seed_property(
            &db_pool,
            owner_id,
            board_id,
            "Size",
            BoardPropertyKind::Choice,
        )
        .await;
        let small_option = new_option(owner_id, property_id, "Small", Some("#22c55e"));
        let large_option = new_option(owner_id, property_id, "Large", Some("#ef4444"));
        PostgresPropertyOptionSource
            .insert_for_new_property(
                &db_pool,
                owner_id,
                property_id,
                &[small_option.clone(), large_option.clone()],
            )
            .await
            .expect("insert should run");

        let rename_save = PostgresPropertyOptionSource
            .update(
                &db_pool,
                board_id,
                small_option.id,
                &OptionChanges {
                    name: Some(String::from("S")),
                    color_change: None,
                },
            )
            .await
            .expect("update should run");
        let clear_save = PostgresPropertyOptionSource
            .update(
                &db_pool,
                board_id,
                large_option.id,
                &OptionChanges {
                    name: None,
                    color_change: Some(None),
                },
            )
            .await
            .expect("update should run");
        let taken_save = PostgresPropertyOptionSource
            .update(
                &db_pool,
                board_id,
                large_option.id,
                &OptionChanges {
                    name: Some(String::from("s")),
                    color_change: None,
                },
            )
            .await
            .expect("update should run");
        let other_board_save = PostgresPropertyOptionSource
            .update(
                &db_pool,
                Uuid::now_v7(),
                large_option.id,
                &OptionChanges::default(),
            )
            .await
            .expect("update should run");

        assert_eq!(rename_save, NameSave::Saved);
        assert_eq!(clear_save, NameSave::Saved);
        assert_eq!(taken_save, NameSave::NameTaken);
        assert_eq!(other_board_save, NameSave::Missing);
        assert_eq!(
            option_rows(&db_pool, property_id).await,
            vec![named("S", Some("#22c55e")), named("Large", None)]
        );
    }

    #[sqlx::test]
    async fn moves_an_option_and_closes_the_gap_a_delete_leaves(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_person_id = seed_user_id(&db_pool, "other@example.com").await;
        let board_id = seed_board_id(&db_pool, owner_id).await;
        let property_id = seed_property(
            &db_pool,
            owner_id,
            board_id,
            "Size",
            BoardPropertyKind::Choice,
        )
        .await;
        let options = [
            new_option(owner_id, property_id, "A", None),
            new_option(owner_id, property_id, "B", None),
            new_option(owner_id, property_id, "C", None),
        ];
        PostgresPropertyOptionSource
            .insert_for_new_property(&db_pool, owner_id, property_id, &options)
            .await
            .expect("insert should run");

        let last_place = PostgresPropertyOptionSource
            .find_place(&db_pool, board_id, options[2].id)
            .await
            .expect("find should run");
        PostgresPropertyOptionSource
            .move_to(&db_pool, property_id, options[2].id, 2, 0)
            .await
            .expect("move should run");
        let after_move = option_rows(&db_pool, property_id).await;
        let removed_position = PostgresPropertyOptionSource
            .delete(&db_pool, property_id, options[0].id)
            .await
            .expect("delete should run")
            .expect("the option should be deleted");
        PostgresPropertyOptionSource
            .close_position_gap(&db_pool, property_id, removed_position)
            .await
            .expect("closing the gap should run");

        assert_eq!(
            last_place,
            Some(OptionPlace {
                property_id,
                position: 2,
                option_count: 3,
            })
        );
        assert_eq!(
            after_move,
            vec![named("C", None), named("A", None), named("B", None)]
        );
        assert_eq!(
            option_rows(&db_pool, property_id).await,
            vec![named("C", None), named("B", None)]
        );
        assert_eq!(
            PostgresPropertyOptionSource
                .find_board_id(&db_pool, owner_id, options[1].id)
                .await
                .expect("find should run"),
            Some(board_id)
        );
        assert_eq!(
            PostgresPropertyOptionSource
                .find_board_id(&db_pool, other_person_id, options[1].id)
                .await
                .expect("find should run"),
            None
        );
    }
}
