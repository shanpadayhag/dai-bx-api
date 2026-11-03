use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::boards::entities::board::{BoardPropertyKind, CardValueContent};
use crate::features::cards::entities::card::ValueSave;

#[derive(Default)]
pub struct PostgresCardValueSource;

impl PostgresCardValueSource {
    pub async fn find_property_kind(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        property_id: Uuid,
    ) -> Result<Option<BoardPropertyKind>, sqlx::Error> {
        sqlx::query_scalar!(
            r#"SELECT kind AS "kind: BoardPropertyKind"
               FROM board_properties
               WHERE id = $2 AND board_id = $1"#,
            board_id,
            property_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn upsert(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        board_id: Uuid,
        card_id: Uuid,
        property_id: Uuid,
        card_value_content: &CardValueContent,
    ) -> Result<ValueSave, sqlx::Error> {
        let (option_id, text_value, number_value) = match card_value_content {
            CardValueContent::Option(option_id) => (Some(*option_id), None, None),
            CardValueContent::Text(text_value) => (None, Some(text_value.as_str()), None),
            CardValueContent::Number(number_value) => (None, None, Some(number_value.as_str())),
        };
        let upsert_result = sqlx::query!(
            "INSERT INTO card_property_values
                 (user_id, board_id, card_id, property_id, option_id, text_value, number_value)
             VALUES ($1, $2, $3, $4, $5, $6, $7::text::numeric)
             ON CONFLICT (card_id, property_id) DO UPDATE
             SET option_id = EXCLUDED.option_id,
                 text_value = EXCLUDED.text_value,
                 number_value = EXCLUDED.number_value,
                 updated_at = now()",
            user_id,
            board_id,
            card_id,
            property_id,
            option_id,
            text_value,
            number_value,
        )
        .execute(executor)
        .await;
        match upsert_result {
            Ok(_) => Ok(ValueSave::Saved),
            Err(sqlx::Error::Database(database_error))
                if database_error.is_foreign_key_violation() =>
            {
                Ok(ValueSave::UnknownOption)
            }
            Err(other_error) => Err(other_error),
        }
    }

    pub async fn delete(
        &self,
        executor: impl PgExecutor<'_>,
        card_id: Uuid,
        property_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM card_property_values WHERE card_id = $1 AND property_id = $2",
            card_id,
            property_id,
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
    use crate::features::board_properties::actions::create_board_property::CreateBoardProperty;
    use crate::features::board_properties::entities::board_property::OptionDraft;
    use crate::features::cards::data_sources::postgres::card_source::tests::{
        seed_board_with_lists, seed_card, seed_user_id,
    };

    struct SeededValueBoard {
        board_id: Uuid,
        card_id: Uuid,
    }

    async fn seed_value_board(db_pool: &PgPool, user_id: Uuid) -> SeededValueBoard {
        let board = seed_board_with_lists(db_pool, user_id, &["Backlog"]).await;
        let board_id = board.board.id;
        let card_id = seed_card(db_pool, user_id, board_id, board.lists[0].id, "Plan").await;
        for (property_name, kind, option_names) in [
            ("Size", BoardPropertyKind::Choice, &["Small", "Large"][..]),
            ("Effort", BoardPropertyKind::Choice, &["Low"][..]),
            ("Hours", BoardPropertyKind::Number, &[][..]),
        ] {
            let option_drafts: Vec<OptionDraft> = option_names
                .iter()
                .map(|option_name| OptionDraft {
                    name: (*option_name).to_owned(),
                    color: None,
                })
                .collect();
            CreateBoardProperty::new(db_pool)
                .execute(user_id, board_id, property_name, kind, &option_drafts)
                .await
                .expect("the property should be created");
        }
        SeededValueBoard { board_id, card_id }
    }

    async fn stored_values(
        db_pool: &PgPool,
        card_id: Uuid,
    ) -> Vec<(Uuid, Option<Uuid>, Option<String>)> {
        sqlx::query_as(
            "SELECT property_id, option_id, number_value::text
             FROM card_property_values
             WHERE card_id = $1
             ORDER BY property_id",
        )
        .bind(card_id)
        .fetch_all(db_pool)
        .await
        .expect("values should be read")
    }

    async fn property_and_option_ids(
        db_pool: &PgPool,
        board_id: Uuid,
        property_name: &str,
    ) -> (Uuid, Vec<Uuid>) {
        let property_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM board_properties WHERE board_id = $1 AND name = $2::text::citext",
        )
        .bind(board_id)
        .bind(property_name)
        .fetch_one(db_pool)
        .await
        .expect("the property should exist");
        let option_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM property_options WHERE property_id = $1 ORDER BY position",
        )
        .bind(property_id)
        .fetch_all(db_pool)
        .await
        .expect("options should be read");
        (property_id, option_ids)
    }

    #[sqlx::test]
    async fn finds_a_property_s_kind_only_on_its_own_board(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded = seed_value_board(&db_pool, owner_id).await;
        let (hours_property_id, _) =
            property_and_option_ids(&db_pool, seeded.board_id, "Hours").await;

        let own_kind = PostgresCardValueSource
            .find_property_kind(&db_pool, seeded.board_id, hours_property_id)
            .await
            .expect("find should run");
        let other_board_kind = PostgresCardValueSource
            .find_property_kind(&db_pool, Uuid::now_v7(), hours_property_id)
            .await
            .expect("find should run");

        assert_eq!(own_kind, Some(BoardPropertyKind::Number));
        assert_eq!(other_board_kind, None);
    }

    #[sqlx::test]
    async fn sets_replaces_and_clears_a_value_keeping_numbers_exact(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded = seed_value_board(&db_pool, owner_id).await;
        let board_id = seeded.board_id;
        let (size_property_id, size_option_ids) =
            property_and_option_ids(&db_pool, board_id, "Size").await;
        let (hours_property_id, _) = property_and_option_ids(&db_pool, board_id, "Hours").await;

        for card_value_content in [
            CardValueContent::Option(size_option_ids[0]),
            CardValueContent::Option(size_option_ids[1]),
        ] {
            let value_save = PostgresCardValueSource
                .upsert(
                    &db_pool,
                    owner_id,
                    board_id,
                    seeded.card_id,
                    size_property_id,
                    &card_value_content,
                )
                .await
                .expect("upsert should run");
            assert_eq!(value_save, ValueSave::Saved);
        }
        PostgresCardValueSource
            .upsert(
                &db_pool,
                owner_id,
                board_id,
                seeded.card_id,
                hours_property_id,
                &CardValueContent::Number(String::from("2.50")),
            )
            .await
            .expect("upsert should run");
        let after_set = stored_values(&db_pool, seeded.card_id).await;
        PostgresCardValueSource
            .delete(&db_pool, seeded.card_id, size_property_id)
            .await
            .expect("delete should run");

        assert_eq!(
            after_set,
            vec![
                (size_property_id, Some(size_option_ids[1]), None),
                (hours_property_id, None, Some(String::from("2.50"))),
            ]
        );
        assert_eq!(
            stored_values(&db_pool, seeded.card_id).await,
            vec![(hours_property_id, None, Some(String::from("2.50")))]
        );
    }

    #[sqlx::test]
    async fn reports_another_property_s_option_as_unknown(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded = seed_value_board(&db_pool, owner_id).await;
        let board_id = seeded.board_id;
        let (size_property_id, _) = property_and_option_ids(&db_pool, board_id, "Size").await;
        let (_, effort_option_ids) = property_and_option_ids(&db_pool, board_id, "Effort").await;

        let value_save = PostgresCardValueSource
            .upsert(
                &db_pool,
                owner_id,
                board_id,
                seeded.card_id,
                size_property_id,
                &CardValueContent::Option(effort_option_ids[0]),
            )
            .await
            .expect("upsert should run");

        assert_eq!(value_save, ValueSave::UnknownOption);
        assert_eq!(stored_values(&db_pool, seeded.card_id).await, Vec::new());
    }
}
