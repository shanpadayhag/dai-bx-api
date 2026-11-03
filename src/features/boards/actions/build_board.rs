use std::collections::HashMap;

use sqlx::PgConnection;

use crate::features::boards::data_sources::postgres::board_source::PostgresBoardSource;
use crate::features::boards::entities::board::{Board, BoardSummary};
use crate::features::boards::error::BoardError;

#[derive(Default)]
pub struct BuildBoard {
    postgres_board_source: PostgresBoardSource,
}

impl BuildBoard {
    pub fn new() -> Self {
        Self {
            postgres_board_source: PostgresBoardSource,
        }
    }

    pub async fn execute(
        &self,
        connection: &mut PgConnection,
        board_summary: BoardSummary,
    ) -> Result<Board, BoardError> {
        let board_id = board_summary.id;
        let lists = self
            .postgres_board_source
            .list_lists(&mut *connection, board_id)
            .await?;
        let mut cards = self
            .postgres_board_source
            .list_cards(&mut *connection, board_id)
            .await?;
        let card_values = self
            .postgres_board_source
            .list_card_values(&mut *connection, board_id)
            .await?;
        let mut properties = self
            .postgres_board_source
            .list_properties(&mut *connection, board_id)
            .await?;
        let property_options = self
            .postgres_board_source
            .list_property_options(&mut *connection, board_id)
            .await?;

        let mut values_by_card = HashMap::with_capacity(cards.len());
        for (card_id, card_value) in card_values {
            values_by_card
                .entry(card_id)
                .or_insert_with(Vec::new)
                .push(card_value);
        }
        for card in &mut cards {
            card.values = values_by_card.remove(&card.id).unwrap_or_default();
        }
        let mut options_by_property = HashMap::with_capacity(properties.len());
        for (property_id, property_option) in property_options {
            options_by_property
                .entry(property_id)
                .or_insert_with(Vec::new)
                .push(property_option);
        }
        for property in &mut properties {
            property.options = options_by_property.remove(&property.id).unwrap_or_default();
        }

        Ok(Board {
            board: board_summary,
            lists,
            cards,
            properties,
        })
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::*;
    use crate::features::boards::data_sources::postgres::board_source::tests::{
        seed_board, seed_user_id,
    };
    use crate::features::boards::entities::board::{
        BoardCard, BoardList, BoardProperty, BoardPropertyKind, CardValue, CardValueContent,
        PropertyOption,
    };
    use crate::features::task_groups::actions::create_task_group::CreateTaskGroup;
    use crate::features::tasks::actions::add_task::AddTask;
    use crate::features::tasks::actions::complete_task::CompleteTask;

    struct SeededBoard {
        board_summary: BoardSummary,
        doing_list_id: Uuid,
        backlog_list_id: Uuid,
        first_card_id: Uuid,
        second_card_id: Uuid,
        size_property_id: Uuid,
        hours_property_id: Uuid,
        link_property_id: Uuid,
        small_option_id: Uuid,
        large_option_id: Uuid,
    }

    async fn seed_board_contents(db_pool: &PgPool, user_id: Uuid) -> SeededBoard {
        let board_summary = seed_board(db_pool, user_id, "Personal").await;
        let seeded_board = SeededBoard {
            board_summary,
            doing_list_id: Uuid::now_v7(),
            backlog_list_id: Uuid::now_v7(),
            first_card_id: Uuid::now_v7(),
            second_card_id: Uuid::now_v7(),
            size_property_id: Uuid::now_v7(),
            hours_property_id: Uuid::now_v7(),
            link_property_id: Uuid::now_v7(),
            small_option_id: Uuid::now_v7(),
            large_option_id: Uuid::now_v7(),
        };
        let board_id = seeded_board.board_summary.id;
        sqlx::query(
            "INSERT INTO board_lists (id, user_id, board_id, position, name)
             VALUES ($1, $3, $4, 1, 'Doing'), ($2, $3, $4, 0, 'Backlog')",
        )
        .bind(seeded_board.doing_list_id)
        .bind(seeded_board.backlog_list_id)
        .bind(user_id)
        .bind(board_id)
        .execute(db_pool)
        .await
        .expect("lists should insert");
        sqlx::query(
            "INSERT INTO cards (id, user_id, board_id, list_id, position, title, notes, due_date, due_time)
             VALUES ($1, $3, $4, $5, 1, 'Second', 'Hidden notes', '2026-11-12', '09:30'),
                    ($2, $3, $4, $5, 0, 'First', '', NULL, NULL)",
        )
        .bind(seeded_board.second_card_id)
        .bind(seeded_board.first_card_id)
        .bind(user_id)
        .bind(board_id)
        .bind(seeded_board.backlog_list_id)
        .execute(db_pool)
        .await
        .expect("cards should insert");
        sqlx::query(
            "INSERT INTO board_properties (id, user_id, board_id, position, name, kind)
             VALUES ($1, $4, $5, 2, 'Link', 'text'),
                    ($2, $4, $5, 0, 'Size', 'choice'),
                    ($3, $4, $5, 1, 'Hours', 'number')",
        )
        .bind(seeded_board.link_property_id)
        .bind(seeded_board.size_property_id)
        .bind(seeded_board.hours_property_id)
        .bind(user_id)
        .bind(board_id)
        .execute(db_pool)
        .await
        .expect("properties should insert");
        sqlx::query(
            "INSERT INTO property_options (id, user_id, property_id, position, name, color)
             VALUES ($1, $3, $4, 1, 'Large', NULL), ($2, $3, $4, 0, 'Small', '#22c55e')",
        )
        .bind(seeded_board.large_option_id)
        .bind(seeded_board.small_option_id)
        .bind(user_id)
        .bind(seeded_board.size_property_id)
        .execute(db_pool)
        .await
        .expect("options should insert");
        sqlx::query(
            "INSERT INTO card_property_values (user_id, board_id, card_id, property_id, option_id, text_value, number_value)
             VALUES ($1, $2, $3, $4, NULL, 'https://example.com', NULL),
                    ($1, $2, $3, $5, NULL, NULL, 2.50),
                    ($1, $2, $3, $6, $7, NULL, NULL)",
        )
        .bind(user_id)
        .bind(board_id)
        .bind(seeded_board.second_card_id)
        .bind(seeded_board.link_property_id)
        .bind(seeded_board.hours_property_id)
        .bind(seeded_board.size_property_id)
        .bind(seeded_board.small_option_id)
        .execute(db_pool)
        .await
        .expect("values should insert");
        seeded_board
    }

    async fn link_tasks_to_card(db_pool: &PgPool, user_id: Uuid, card_id: Uuid, done_count: usize) {
        let task_group = CreateTaskGroup::new()
            .execute(db_pool, user_id, "Today")
            .await
            .expect("the group should be created");
        for task_name in ["Draft", "Review", "Send"] {
            AddTask::new(db_pool)
                .execute(user_id, task_group.id, task_name, None)
                .await
                .expect("the task should be added");
        }
        let task_ids: Vec<Uuid> =
            sqlx::query_scalar("UPDATE tasks SET card_id = $1 WHERE group_id = $2 RETURNING id")
                .bind(card_id)
                .bind(task_group.id)
                .fetch_all(db_pool)
                .await
                .expect("tasks should link");
        for task_id in task_ids.into_iter().take(done_count) {
            CompleteTask::new(db_pool)
                .execute(user_id, task_id)
                .await
                .expect("the task should be completed");
        }
    }

    #[sqlx::test]
    async fn builds_a_board_from_every_table_without_notes(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded_board = seed_board_contents(&db_pool, owner_id).await;
        link_tasks_to_card(&db_pool, owner_id, seeded_board.second_card_id, 1).await;
        let mut board_connection = db_pool.acquire().await.expect("a connection");

        let board = BuildBoard::new()
            .execute(&mut board_connection, seeded_board.board_summary.clone())
            .await
            .expect("the board should build");

        assert_eq!(board.board, seeded_board.board_summary);
        assert_eq!(
            board.lists,
            vec![
                BoardList {
                    id: seeded_board.backlog_list_id,
                    name: String::from("Backlog"),
                    position: 0,
                },
                BoardList {
                    id: seeded_board.doing_list_id,
                    name: String::from("Doing"),
                    position: 1,
                },
            ]
        );
        assert_eq!(
            board.cards,
            vec![
                BoardCard {
                    id: seeded_board.first_card_id,
                    list_id: seeded_board.backlog_list_id,
                    position: 0,
                    title: String::from("First"),
                    due_date: None,
                    due_time: None,
                    values: Vec::new(),
                    linked_task_count: 0,
                    done_linked_task_count: 0,
                },
                BoardCard {
                    id: seeded_board.second_card_id,
                    list_id: seeded_board.backlog_list_id,
                    position: 1,
                    title: String::from("Second"),
                    due_date: Some(String::from("2026-11-12")),
                    due_time: Some(String::from("09:30")),
                    values: vec![
                        CardValue {
                            property_id: seeded_board.size_property_id,
                            content: CardValueContent::Option(seeded_board.small_option_id),
                        },
                        CardValue {
                            property_id: seeded_board.hours_property_id,
                            content: CardValueContent::Number(String::from("2.50")),
                        },
                        CardValue {
                            property_id: seeded_board.link_property_id,
                            content: CardValueContent::Text(String::from("https://example.com")),
                        },
                    ],
                    linked_task_count: 3,
                    done_linked_task_count: 1,
                },
            ]
        );
        assert_eq!(
            board.properties,
            vec![
                BoardProperty {
                    id: seeded_board.size_property_id,
                    name: String::from("Size"),
                    kind: BoardPropertyKind::Choice,
                    position: 0,
                    options: vec![
                        PropertyOption {
                            id: seeded_board.small_option_id,
                            name: String::from("Small"),
                            color: Some(String::from("#22c55e")),
                            position: 0,
                        },
                        PropertyOption {
                            id: seeded_board.large_option_id,
                            name: String::from("Large"),
                            color: None,
                            position: 1,
                        },
                    ],
                },
                BoardProperty {
                    id: seeded_board.hours_property_id,
                    name: String::from("Hours"),
                    kind: BoardPropertyKind::Number,
                    position: 1,
                    options: Vec::new(),
                },
                BoardProperty {
                    id: seeded_board.link_property_id,
                    name: String::from("Link"),
                    kind: BoardPropertyKind::Text,
                    position: 2,
                    options: Vec::new(),
                },
            ]
        );
    }

    #[sqlx::test]
    async fn builds_only_the_asked_for_board(db_pool: PgPool) {
        let owner_id = seed_user_id(&db_pool, "owner@example.com").await;
        seed_board_contents(&db_pool, owner_id).await;
        let empty_board = seed_board(&db_pool, owner_id, "Studies").await;
        let mut board_connection = db_pool.acquire().await.expect("a connection");

        let board = BuildBoard::new()
            .execute(&mut board_connection, empty_board.clone())
            .await
            .expect("the board should build");

        assert_eq!(
            board,
            Board {
                board: empty_board,
                lists: Vec::new(),
                cards: Vec::new(),
                properties: Vec::new(),
            }
        );
    }
}
