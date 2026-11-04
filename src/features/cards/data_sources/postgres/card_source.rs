use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::boards::entities::board::{BoardCard, CardValue, CardValueContent};
use crate::features::cards::entities::card::{
    CardChanges, CardDue, CardPlace, CardWithNotes, LinkedTask, NewCard,
};

#[derive(Default)]
pub struct PostgresCardSource;

impl PostgresCardSource {
    pub async fn find_board_id(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        card_id: Uuid,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar!(
            "SELECT board_id FROM cards WHERE id = $1 AND user_id = $2",
            card_id,
            user_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn find_title(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        card_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar!(
            "SELECT title FROM cards WHERE id = $1 AND user_id = $2",
            card_id,
            user_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn insert_last(
        &self,
        executor: impl PgExecutor<'_>,
        new_card: &NewCard,
        card_notes: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "INSERT INTO cards (id, user_id, board_id, list_id, position, title, notes)
             SELECT $1, $2, $3, $4, coalesce(max(position) + 1, 0), $5, $6
             FROM cards
             WHERE list_id = $4",
            new_card.id,
            new_card.user_id,
            new_card.board_id,
            new_card.list_id,
            new_card.title,
            card_notes,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn find_with_notes(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        card_id: Uuid,
    ) -> Result<Option<CardWithNotes>, sqlx::Error> {
        let card_row = sqlx::query!(
            r#"SELECT id,
                      list_id,
                      position,
                      title,
                      notes,
                      to_char(due_date, 'YYYY-MM-DD') AS due_date,
                      to_char(due_time, 'HH24:MI') AS due_time,
                      (SELECT count(*)
                       FROM calendar_events
                       WHERE calendar_events.card_id = cards.id) AS "tracked_event_count!"
               FROM cards
               WHERE id = $1 AND user_id = $2"#,
            card_id,
            user_id,
        )
        .fetch_optional(executor)
        .await?;
        Ok(card_row.map(|card_row| CardWithNotes {
            card: BoardCard {
                id: card_row.id,
                list_id: card_row.list_id,
                position: card_row.position,
                title: card_row.title,
                due_date: card_row.due_date,
                due_time: card_row.due_time,
                values: Vec::new(),
                linked_task_count: 0,
                done_linked_task_count: 0,
            },
            notes: card_row.notes,
            tracked_event_count: card_row.tracked_event_count,
        }))
    }

    pub async fn list_values(
        &self,
        executor: impl PgExecutor<'_>,
        card_id: Uuid,
    ) -> Result<Vec<CardValue>, sqlx::Error> {
        let value_rows = sqlx::query!(
            r#"SELECT card_property_values.property_id,
                      card_property_values.option_id,
                      card_property_values.text_value,
                      card_property_values.number_value::text AS number_value
               FROM card_property_values
               JOIN board_properties ON board_properties.id = card_property_values.property_id
               WHERE card_property_values.card_id = $1
               ORDER BY board_properties.position"#,
            card_id,
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
                Ok(CardValue {
                    property_id: value_row.property_id,
                    content,
                })
            })
            .collect()
    }

    pub async fn list_linked_tasks(
        &self,
        executor: impl PgExecutor<'_>,
        card_id: Uuid,
    ) -> Result<Vec<LinkedTask>, sqlx::Error> {
        sqlx::query_as!(
            LinkedTask,
            r#"SELECT tasks.id,
                      tasks.name,
                      tasks.completed_at IS NOT NULL AS "is_done!",
                      task_groups.id AS group_id,
                      task_groups.name AS group_name
               FROM tasks
               JOIN task_groups ON task_groups.id = tasks.group_id
               WHERE tasks.card_id = $1
               ORDER BY task_groups.created_at, task_groups.id, tasks.position, tasks.id"#,
            card_id,
        )
        .fetch_all(executor)
        .await
    }

    pub async fn find_due(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        card_id: Uuid,
    ) -> Result<Option<CardDue>, sqlx::Error> {
        sqlx::query_as!(
            CardDue,
            r#"SELECT to_char(due_date, 'YYYY-MM-DD') AS due_date,
                      to_char(due_time, 'HH24:MI') AS due_time
               FROM cards
               WHERE id = $2 AND board_id = $1"#,
            board_id,
            card_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn update(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        card_id: Uuid,
        card_changes: &CardChanges,
        changed_due: &CardDue,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE cards
             SET title = coalesce($3, title),
                 notes = coalesce($4, notes),
                 due_date = $5::text::date,
                 due_time = $6::text::time,
                 updated_at = now()
             WHERE id = $2 AND board_id = $1",
            board_id,
            card_id,
            card_changes.title,
            card_changes.notes,
            changed_due.due_date,
            changed_due.due_time,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn find_place(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        card_id: Uuid,
    ) -> Result<Option<CardPlace>, sqlx::Error> {
        sqlx::query_as!(
            CardPlace,
            "SELECT list_id, position FROM cards WHERE id = $2 AND board_id = $1",
            board_id,
            card_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn count_in_list(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        list_id: Uuid,
    ) -> Result<Option<i32>, sqlx::Error> {
        sqlx::query_scalar!(
            r#"SELECT (SELECT count(*)::integer FROM cards WHERE list_id = $2) AS "card_count!"
               FROM board_lists
               WHERE id = $2 AND board_id = $1"#,
            board_id,
            list_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn move_within_list(
        &self,
        executor: impl PgExecutor<'_>,
        card_id: Uuid,
        current_place: CardPlace,
        target_position: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE cards
             SET position = CASE
                     WHEN id = $2 THEN $4::integer
                     WHEN $3::integer < $4::integer THEN position - 1
                     ELSE position + 1
                 END,
                 updated_at = CASE WHEN id = $2 THEN now() ELSE updated_at END
             WHERE list_id = $1
               AND (id = $2 OR position BETWEEN least($3::integer, $4::integer)
                                            AND greatest($3::integer, $4::integer))",
            current_place.list_id,
            card_id,
            current_place.position,
            target_position,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn move_to_other_list(
        &self,
        executor: impl PgExecutor<'_>,
        card_id: Uuid,
        current_place: CardPlace,
        target_place: CardPlace,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE cards
             SET list_id = CASE WHEN id = $1 THEN $4 ELSE list_id END,
                 position = CASE
                     WHEN id = $1 THEN $5::integer
                     WHEN list_id = $2 THEN position - 1
                     ELSE position + 1
                 END,
                 updated_at = CASE WHEN id = $1 THEN now() ELSE updated_at END
             WHERE id = $1
                OR (list_id = $2 AND position > $3::integer)
                OR (list_id = $4 AND position >= $5::integer)",
            card_id,
            current_place.list_id,
            current_place.position,
            target_place.list_id,
            target_place.position,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn delete(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
        card_id: Uuid,
    ) -> Result<Option<CardPlace>, sqlx::Error> {
        sqlx::query_as!(
            CardPlace,
            "DELETE FROM cards WHERE id = $2 AND board_id = $1 RETURNING list_id, position",
            board_id,
            card_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn close_position_gap(
        &self,
        executor: impl PgExecutor<'_>,
        removed_place: CardPlace,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE cards SET position = position - 1 WHERE list_id = $1 AND position > $2",
            removed_place.list_id,
            removed_place.position,
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
    use crate::features::board_lists::actions::create_board_list::CreateBoardList;
    use crate::features::boards::actions::create_board::CreateBoard;
    use crate::features::boards::entities::board::Board;
    use crate::features::cards::entities::card::NEW_CARD_NOTES;
    use crate::features::task_groups::actions::create_task_group::CreateTaskGroup;
    use crate::features::tasks::actions::add_task::AddTask;
    use crate::features::tasks::actions::complete_task::CompleteTask;
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

    pub async fn seed_board_with_lists(
        db_pool: &PgPool,
        user_id: Uuid,
        list_names: &[&str],
    ) -> Board {
        let mut board = CreateBoard::new()
            .execute(db_pool, user_id, "Personal")
            .await
            .expect("the board should be created");
        for list_name in list_names {
            board = CreateBoardList::new(db_pool)
                .execute(user_id, board.board.id, list_name)
                .await
                .expect("the list should be created");
        }
        board
    }

    pub async fn seed_card(
        db_pool: &PgPool,
        user_id: Uuid,
        board_id: Uuid,
        list_id: Uuid,
        title: &str,
    ) -> Uuid {
        let card_id = Uuid::now_v7();
        PostgresCardSource
            .insert_last(
                db_pool,
                &NewCard {
                    id: card_id,
                    user_id,
                    board_id,
                    list_id,
                    title: title.to_owned(),
                },
                NEW_CARD_NOTES,
            )
            .await
            .expect("insert should run");
        card_id
    }

    async fn titles_in_list(db_pool: &PgPool, list_id: Uuid) -> Vec<(String, i32)> {
        sqlx::query_as("SELECT title, position FROM cards WHERE list_id = $1 ORDER BY position")
            .bind(list_id)
            .fetch_all(db_pool)
            .await
            .expect("cards should be read")
    }

    fn places(expected_titles: &[&str]) -> Vec<(String, i32)> {
        expected_titles
            .iter()
            .zip(0..)
            .map(|(card_title, position)| ((*card_title).to_owned(), position))
            .collect()
    }

    #[sqlx::test]
    async fn reads_the_title_of_only_the_owner_s_card(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_person_id = seed_user_id(&db_pool, "other@example.com").await;
        let board = seed_board_with_lists(&db_pool, owner_id, &["Backlog"]).await;
        let card_id = seed_card(
            &db_pool,
            owner_id,
            board.board.id,
            board.lists[0].id,
            "Plan",
        )
        .await;

        let owner_title = PostgresCardSource
            .find_title(&db_pool, owner_id, card_id)
            .await
            .expect("the read should run");
        let foreign_title = PostgresCardSource
            .find_title(&db_pool, other_person_id, card_id)
            .await
            .expect("the read should run");

        assert_eq!(owner_title.as_deref(), Some("Plan"));
        assert_eq!(foreign_title, None);
    }

    #[sqlx::test]
    async fn appends_each_card_after_its_list_s_last(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let board = seed_board_with_lists(&db_pool, owner_id, &["Backlog", "Done"]).await;
        let (backlog_list_id, done_list_id) = (board.lists[0].id, board.lists[1].id);
        seed_card(
            &db_pool,
            owner_id,
            board.board.id,
            done_list_id,
            "Elsewhere",
        )
        .await;

        for card_title in ["A", "B"] {
            seed_card(
                &db_pool,
                owner_id,
                board.board.id,
                backlog_list_id,
                card_title,
            )
            .await;
        }

        assert_eq!(
            titles_in_list(&db_pool, backlog_list_id).await,
            places(&["A", "B"])
        );
    }

    #[sqlx::test]
    async fn moves_a_card_into_another_list_closing_and_opening_gaps(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let board = seed_board_with_lists(&db_pool, owner_id, &["Backlog", "Done"]).await;
        let (backlog_list_id, done_list_id) = (board.lists[0].id, board.lists[1].id);
        let first_card_id =
            seed_card(&db_pool, owner_id, board.board.id, backlog_list_id, "A").await;
        seed_card(&db_pool, owner_id, board.board.id, backlog_list_id, "B").await;
        for card_title in ["X", "Y"] {
            seed_card(&db_pool, owner_id, board.board.id, done_list_id, card_title).await;
        }

        PostgresCardSource
            .move_to_other_list(
                &db_pool,
                first_card_id,
                CardPlace {
                    list_id: backlog_list_id,
                    position: 0,
                },
                CardPlace {
                    list_id: done_list_id,
                    position: 1,
                },
            )
            .await
            .expect("move should run");

        assert_eq!(
            titles_in_list(&db_pool, backlog_list_id).await,
            places(&["B"])
        );
        assert_eq!(
            titles_in_list(&db_pool, done_list_id).await,
            places(&["X", "A", "Y"])
        );
    }

    #[sqlx::test]
    async fn counts_cards_only_in_a_list_on_the_given_board(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let board = seed_board_with_lists(&db_pool, owner_id, &["Backlog"]).await;
        let other_board = seed_board_with_lists(&db_pool, owner_id, &["Elsewhere"]).await;
        seed_card(&db_pool, owner_id, board.board.id, board.lists[0].id, "A").await;

        let card_count = PostgresCardSource
            .count_in_list(&db_pool, board.board.id, board.lists[0].id)
            .await
            .expect("count should run");
        let count_from_another_board = PostgresCardSource
            .count_in_list(&db_pool, other_board.board.id, board.lists[0].id)
            .await
            .expect("count should run");

        assert_eq!(card_count, Some(1));
        assert_eq!(count_from_another_board, None);
    }

    #[sqlx::test]
    async fn lists_linked_tasks_with_their_group_and_state(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let board = seed_board_with_lists(&db_pool, owner_id, &["Backlog"]).await;
        let card_id = seed_card(
            &db_pool,
            owner_id,
            board.board.id,
            board.lists[0].id,
            "Plan",
        )
        .await;
        let task_group = CreateTaskGroup::new()
            .execute(&db_pool, owner_id, "Today")
            .await
            .expect("the group should be created");
        for task_name in ["Draft", "Send"] {
            AddTask::new(&db_pool)
                .execute(owner_id, task_group.id, task_name, None)
                .await
                .expect("the task should be added");
        }
        let linked_task_ids: Vec<Uuid> =
            sqlx::query_scalar("UPDATE tasks SET card_id = $1 WHERE group_id = $2 RETURNING id")
                .bind(card_id)
                .bind(task_group.id)
                .fetch_all(&db_pool)
                .await
                .expect("tasks should link");
        let draft_task_id = *linked_task_ids.iter().min().expect("a linked task");
        CompleteTask::new(&db_pool)
            .execute(owner_id, draft_task_id)
            .await
            .expect("the task should be completed");

        let linked_tasks = PostgresCardSource
            .list_linked_tasks(&db_pool, card_id)
            .await
            .expect("listing should run");

        assert_eq!(
            linked_tasks
                .iter()
                .map(|linked_task| (
                    linked_task.name.as_str(),
                    linked_task.is_done,
                    linked_task.group_name.as_str()
                ))
                .collect::<Vec<_>>(),
            vec![("Draft", true, "Today"), ("Send", false, "Today")]
        );
        assert!(
            linked_tasks
                .iter()
                .all(|linked_task| linked_task.group_id == task_group.id)
        );
    }

    #[sqlx::test]
    async fn stores_a_due_date_and_time_and_reads_them_back_as_text(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let board = seed_board_with_lists(&db_pool, owner_id, &["Backlog"]).await;
        let card_id = seed_card(
            &db_pool,
            owner_id,
            board.board.id,
            board.lists[0].id,
            "Plan",
        )
        .await;
        let changed_due = CardDue {
            due_date: Some(String::from("2026-11-05")),
            due_time: Some(String::from("09:05")),
        };

        PostgresCardSource
            .update(
                &db_pool,
                board.board.id,
                card_id,
                &CardChanges::default(),
                &changed_due,
            )
            .await
            .expect("update should run");

        assert_eq!(
            PostgresCardSource
                .find_due(&db_pool, board.board.id, card_id)
                .await
                .expect("find should run"),
            Some(changed_due)
        );
    }
}
