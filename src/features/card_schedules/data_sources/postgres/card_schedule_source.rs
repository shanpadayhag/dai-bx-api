use jiff::civil::DateTime;
use sqlx::PgExecutor;
use sqlx::types::Json;
use uuid::Uuid;

use crate::core::recurrence::RepeatRule;
use crate::core::utc_instant::UtcInstant;
use crate::features::boards::entities::board::NextMove;
use crate::features::card_schedules::entities::card_schedule::{
    CardSchedule, DeletedCards, DueSchedule, NewCardSchedule, ScheduleBlock, ScheduleSettings,
    ScheduleTiming, ScheduleWrite,
};

#[derive(Default)]
pub struct PostgresCardScheduleSource;

struct CardScheduleRow {
    id: Uuid,
    card_id: Uuid,
    board_id: Uuid,
    card_title: String,
    target_list_id: Option<Uuid>,
    target_list_name: Option<String>,
    time_zone: String,
    local_start_text: String,
    repeat_rule: Option<Json<RepeatRule>>,
    block_minutes: Option<i32>,
    block_category_id: Option<Uuid>,
    paused: bool,
    next_run_at_millis: Option<i64>,
}

impl TryFrom<CardScheduleRow> for CardSchedule {
    type Error = sqlx::Error;

    fn try_from(schedule_row: CardScheduleRow) -> Result<Self, Self::Error> {
        let local_start = schedule_row
            .local_start_text
            .parse::<DateTime>()
            .map_err(|parse_error| sqlx::Error::Decode(Box::new(parse_error)))?;
        Ok(Self {
            id: schedule_row.id,
            card_id: schedule_row.card_id,
            board_id: schedule_row.board_id,
            card_title: schedule_row.card_title,
            target_list_id: schedule_row.target_list_id,
            target_list_name: schedule_row.target_list_name,
            timing: ScheduleTiming {
                time_zone: schedule_row.time_zone,
                local_start,
                repeat_rule: schedule_row
                    .repeat_rule
                    .map(|Json(repeat_rule)| repeat_rule),
            },
            block: schedule_row.block_minutes.map(|minutes| ScheduleBlock {
                minutes,
                category_id: schedule_row.block_category_id,
            }),
            is_paused: schedule_row.paused,
            next_run_at: schedule_row
                .next_run_at_millis
                .map(UtcInstant::from_unix_millis),
        })
    }
}

fn write_outcome<T>(write_result: Result<T, sqlx::Error>) -> Result<ScheduleWrite, sqlx::Error> {
    match write_result {
        Ok(_) => Ok(ScheduleWrite::Written),
        Err(sqlx::Error::Database(database_error)) if database_error.is_foreign_key_violation() => {
            Ok(ScheduleWrite::UnknownLink)
        }
        Err(other_error) => Err(other_error),
    }
}

fn rows_into_schedules(
    schedule_rows: Vec<CardScheduleRow>,
) -> Result<Vec<CardSchedule>, sqlx::Error> {
    schedule_rows
        .into_iter()
        .map(CardSchedule::try_from)
        .collect()
}

impl PostgresCardScheduleSource {
    pub async fn insert(
        &self,
        executor: impl PgExecutor<'_>,
        new_schedule: &NewCardSchedule,
    ) -> Result<ScheduleWrite, sqlx::Error> {
        let schedule_settings = &new_schedule.settings;
        let insert_result = sqlx::query!(
            "INSERT INTO card_schedules
                 (id, user_id, board_id, card_id, kind, target_list_id, time_zone, local_start,
                  repeat_rule, block_minutes, block_category_id, next_run_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8::text::timestamp, $9, $10, $11,
                     'epoch'::timestamptz + $12::bigint * interval '1 millisecond')",
            new_schedule.id,
            new_schedule.user_id,
            new_schedule.board_id,
            new_schedule.card_id,
            schedule_settings.timing.kind().as_str(),
            schedule_settings.target_list_id,
            schedule_settings.timing.time_zone,
            schedule_settings.timing.local_start.to_string(),
            schedule_settings.timing.repeat_rule.as_ref().map(Json) as _,
            schedule_settings
                .block
                .map(|schedule_block| schedule_block.minutes),
            schedule_settings
                .block
                .and_then(|schedule_block| schedule_block.category_id),
            new_schedule.next_run_at.unix_millis(),
        )
        .execute(executor)
        .await;
        write_outcome(insert_result)
    }

    pub async fn update(
        &self,
        executor: impl PgExecutor<'_>,
        schedule_id: Uuid,
        schedule_settings: &ScheduleSettings,
        next_run_at: UtcInstant,
    ) -> Result<ScheduleWrite, sqlx::Error> {
        let update_result = sqlx::query!(
            "UPDATE card_schedules
             SET kind = $2,
                 target_list_id = $3,
                 time_zone = $4,
                 local_start = $5::text::timestamp,
                 repeat_rule = $6,
                 block_minutes = $7,
                 block_category_id = $8,
                 next_run_at = 'epoch'::timestamptz + $9::bigint * interval '1 millisecond',
                 updated_at = now()
             WHERE id = $1",
            schedule_id,
            schedule_settings.timing.kind().as_str(),
            schedule_settings.target_list_id,
            schedule_settings.timing.time_zone,
            schedule_settings.timing.local_start.to_string(),
            schedule_settings.timing.repeat_rule.as_ref().map(Json) as _,
            schedule_settings
                .block
                .map(|schedule_block| schedule_block.minutes),
            schedule_settings
                .block
                .and_then(|schedule_block| schedule_block.category_id),
            next_run_at.unix_millis(),
        )
        .execute(executor)
        .await;
        write_outcome(update_result)
    }

    pub async fn set_paused(
        &self,
        executor: impl PgExecutor<'_>,
        schedule_id: Uuid,
        is_paused: bool,
        next_run_at: Option<UtcInstant>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE card_schedules
             SET paused = $2,
                 paused_by_archive = paused_by_archive AND $2,
                 next_run_at = 'epoch'::timestamptz + $3::bigint * interval '1 millisecond',
                 updated_at = now()
             WHERE id = $1",
            schedule_id,
            is_paused,
            next_run_at.map(UtcInstant::unix_millis),
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn set_next_run_at(
        &self,
        executor: impl PgExecutor<'_>,
        schedule_id: Uuid,
        next_run_at: Option<UtcInstant>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE card_schedules
             SET next_run_at = 'epoch'::timestamptz + $2::bigint * interval '1 millisecond',
                 updated_at = now()
             WHERE id = $1",
            schedule_id,
            next_run_at.map(UtcInstant::unix_millis),
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn find_card_id(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        schedule_id: Uuid,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar!(
            "SELECT card_id FROM card_schedules WHERE id = $2 AND user_id = $1",
            user_id,
            schedule_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn find_locked(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        schedule_id: Uuid,
    ) -> Result<Option<CardSchedule>, sqlx::Error> {
        let schedule_row = sqlx::query_as!(
            CardScheduleRow,
            r#"SELECT card_schedules.id,
                      card_schedules.card_id,
                      card_schedules.board_id,
                      cards.title AS card_title,
                      card_schedules.target_list_id,
                      board_lists.name AS "target_list_name?",
                      card_schedules.time_zone,
                      to_char(card_schedules.local_start, 'YYYY-MM-DD"T"HH24:MI:SS') AS "local_start_text!",
                      card_schedules.repeat_rule AS "repeat_rule: Json<RepeatRule>",
                      card_schedules.block_minutes,
                      card_schedules.block_category_id,
                      card_schedules.paused,
                      (extract(epoch FROM card_schedules.next_run_at) * 1000)::bigint AS next_run_at_millis
               FROM card_schedules
               JOIN cards ON cards.id = card_schedules.card_id
               LEFT JOIN board_lists ON board_lists.id = card_schedules.target_list_id
               WHERE card_schedules.id = $2 AND card_schedules.user_id = $1
               FOR UPDATE OF card_schedules"#,
            user_id,
            schedule_id,
        )
        .fetch_optional(executor)
        .await?;
        schedule_row.map(CardSchedule::try_from).transpose()
    }

    pub async fn list_for_card(
        &self,
        executor: impl PgExecutor<'_>,
        card_id: Uuid,
    ) -> Result<Vec<CardSchedule>, sqlx::Error> {
        let schedule_rows = sqlx::query_as!(
            CardScheduleRow,
            r#"SELECT card_schedules.id,
                      card_schedules.card_id,
                      card_schedules.board_id,
                      cards.title AS card_title,
                      card_schedules.target_list_id,
                      board_lists.name AS "target_list_name?",
                      card_schedules.time_zone,
                      to_char(card_schedules.local_start, 'YYYY-MM-DD"T"HH24:MI:SS') AS "local_start_text!",
                      card_schedules.repeat_rule AS "repeat_rule: Json<RepeatRule>",
                      card_schedules.block_minutes,
                      card_schedules.block_category_id,
                      card_schedules.paused,
                      (extract(epoch FROM card_schedules.next_run_at) * 1000)::bigint AS next_run_at_millis
               FROM card_schedules
               JOIN cards ON cards.id = card_schedules.card_id
               LEFT JOIN board_lists ON board_lists.id = card_schedules.target_list_id
               WHERE card_schedules.card_id = $1
               ORDER BY card_schedules.id"#,
            card_id,
        )
        .fetch_all(executor)
        .await?;
        rows_into_schedules(schedule_rows)
    }

    pub async fn list_active(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Vec<CardSchedule>, sqlx::Error> {
        let schedule_rows = sqlx::query_as!(
            CardScheduleRow,
            r#"SELECT card_schedules.id,
                      card_schedules.card_id,
                      card_schedules.board_id,
                      cards.title AS card_title,
                      card_schedules.target_list_id,
                      board_lists.name AS "target_list_name?",
                      card_schedules.time_zone,
                      to_char(card_schedules.local_start, 'YYYY-MM-DD"T"HH24:MI:SS') AS "local_start_text!",
                      card_schedules.repeat_rule AS "repeat_rule: Json<RepeatRule>",
                      card_schedules.block_minutes,
                      card_schedules.block_category_id,
                      card_schedules.paused,
                      (extract(epoch FROM card_schedules.next_run_at) * 1000)::bigint AS next_run_at_millis
               FROM card_schedules
               JOIN cards ON cards.id = card_schedules.card_id
               JOIN board_lists ON board_lists.id = card_schedules.target_list_id
               WHERE card_schedules.user_id = $1
                 AND NOT card_schedules.paused
                 AND card_schedules.next_run_at IS NOT NULL
               ORDER BY card_schedules.id"#,
            user_id,
        )
        .fetch_all(executor)
        .await?;
        rows_into_schedules(schedule_rows)
    }

    pub async fn count_for_card(
        &self,
        executor: impl PgExecutor<'_>,
        card_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar!(
            r#"SELECT count(*) AS "schedule_count!" FROM card_schedules WHERE card_id = $1"#,
            card_id,
        )
        .fetch_one(executor)
        .await
    }

    pub async fn delete(
        &self,
        executor: impl PgExecutor<'_>,
        schedule_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let delete_result = sqlx::query!("DELETE FROM card_schedules WHERE id = $1", schedule_id)
            .execute(executor)
            .await?;
        Ok(delete_result.rows_affected() == 1)
    }

    pub async fn list_due(
        &self,
        executor: impl PgExecutor<'_>,
        now: UtcInstant,
        due_limit: i64,
    ) -> Result<Vec<DueSchedule>, sqlx::Error> {
        sqlx::query_as!(
            DueSchedule,
            "SELECT id AS schedule_id, user_id
             FROM card_schedules
             WHERE NOT paused
               AND target_list_id IS NOT NULL
               AND next_run_at <= 'epoch'::timestamptz + $1::bigint * interval '1 millisecond'
             ORDER BY next_run_at, id
             LIMIT $2",
            now.unix_millis(),
            due_limit,
        )
        .fetch_all(executor)
        .await
    }

    pub async fn pause_into_list(
        &self,
        executor: impl PgExecutor<'_>,
        list_id: Uuid,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        sqlx::query_scalar!(
            "UPDATE card_schedules SET paused = true, updated_at = now()
             WHERE target_list_id = $1 AND NOT paused
             RETURNING id",
            list_id,
        )
        .fetch_all(executor)
        .await
    }

    pub async fn pause_for_archive(
        &self,
        executor: impl PgExecutor<'_>,
        card_id: Uuid,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        sqlx::query_scalar!(
            "UPDATE card_schedules
             SET paused = true, paused_by_archive = true, updated_at = now()
             WHERE card_id = $1 AND NOT paused
             RETURNING id",
            card_id,
        )
        .fetch_all(executor)
        .await
    }

    pub async fn list_ids_paused_by_archive(
        &self,
        executor: impl PgExecutor<'_>,
        card_id: Uuid,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        sqlx::query_scalar!(
            "SELECT id FROM card_schedules WHERE card_id = $1 AND paused_by_archive ORDER BY id",
            card_id,
        )
        .fetch_all(executor)
        .await
    }

    pub async fn list_ids_of_deleted_cards(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        deleted_cards: DeletedCards,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        let (card_id, list_id, board_id) = match deleted_cards {
            DeletedCards::Card(card_id) => (Some(card_id), None, None),
            DeletedCards::BoardList(list_id) => (None, Some(list_id), None),
            DeletedCards::Board(board_id) => (None, None, Some(board_id)),
        };
        sqlx::query_scalar!(
            "SELECT card_schedules.id
             FROM card_schedules
             JOIN cards ON cards.id = card_schedules.card_id
             WHERE card_schedules.user_id = $1
               AND ($2::uuid IS NULL OR cards.id = $2)
               AND ($3::uuid IS NULL OR cards.list_id = $3)
               AND ($4::uuid IS NULL OR cards.board_id = $4)",
            user_id,
            card_id,
            list_id,
            board_id,
        )
        .fetch_all(executor)
        .await
    }

    pub async fn list_next_moves(
        &self,
        executor: impl PgExecutor<'_>,
        board_id: Uuid,
    ) -> Result<Vec<(Uuid, NextMove)>, sqlx::Error> {
        let next_move_rows = sqlx::query!(
            r#"SELECT DISTINCT ON (card_schedules.card_id)
                      card_schedules.card_id,
                      (extract(epoch FROM card_schedules.next_run_at) * 1000)::bigint AS "next_run_at_millis!",
                      board_lists.name AS list_name
               FROM card_schedules
               JOIN board_lists ON board_lists.id = card_schedules.target_list_id
               WHERE card_schedules.board_id = $1
                 AND NOT card_schedules.paused
                 AND card_schedules.next_run_at IS NOT NULL
               ORDER BY card_schedules.card_id, card_schedules.next_run_at, card_schedules.id"#,
            board_id,
        )
        .fetch_all(executor)
        .await?;
        Ok(next_move_rows
            .into_iter()
            .map(|next_move_row| {
                (
                    next_move_row.card_id,
                    NextMove {
                        at: UtcInstant::from_unix_millis(next_move_row.next_run_at_millis),
                        list_name: next_move_row.list_name,
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
    use crate::features::board_lists::actions::create_board_list::CreateBoardList;
    use crate::features::boards::actions::create_board::CreateBoard;
    use crate::features::card_schedules::entities::card_schedule::ScheduleKind;
    use crate::features::cards::actions::create_card::CreateCard;
    use crate::features::categories::actions::create_category::CreateCategory;
    use crate::features::users::actions::create_user::CreateUser;
    use crate::features::users::entities::user::NewUser;

    pub struct SeededCard {
        pub user_id: Uuid,
        pub board_id: Uuid,
        pub card_id: Uuid,
        pub to_do_list_id: Uuid,
        pub doing_list_id: Uuid,
    }

    pub fn instant(instant_text: &str) -> UtcInstant {
        UtcInstant::parse(instant_text).expect("a valid test time")
    }

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

    pub async fn seed_category_id(db_pool: &PgPool, user_id: Uuid) -> Uuid {
        CreateCategory::new()
            .execute(db_pool, user_id, "Focus", "#5b7fa6")
            .await
            .expect("the category should be created")
            .id
    }

    pub async fn seed_card(db_pool: &PgPool, user_id: Uuid) -> SeededCard {
        let board = CreateBoard::new()
            .execute(db_pool, user_id, "Projects")
            .await
            .expect("the board should be created");
        let board = CreateBoardList::new(db_pool)
            .execute(user_id, board.board.id, "To Do")
            .await
            .expect("the list should be created");
        let board = CreateBoardList::new(db_pool)
            .execute(user_id, board.board.id, "Doing")
            .await
            .expect("the list should be created");
        let to_do_list_id = board.lists[0].id;
        let doing_list_id = board.lists[1].id;
        let board = CreateCard::new(db_pool)
            .execute(user_id, doing_list_id, "Weekly review")
            .await
            .expect("the card should be created");
        SeededCard {
            user_id,
            board_id: board.board.id,
            card_id: board.cards[0].id,
            to_do_list_id,
            doing_list_id,
        }
    }

    pub fn weekly_settings(target_list_id: Uuid) -> ScheduleSettings {
        ScheduleSettings {
            target_list_id,
            timing: ScheduleTiming::from_input(
                ScheduleKind::Repeating,
                "2030-01-07T18:00",
                "Asia/Manila",
                None,
                Some(
                    serde_json::from_str(
                        r#"{"frequency":"weekly","interval":1,"weekdays":["mon"],"ends":{"never":true}}"#,
                    )
                    .expect("a valid rule"),
                ),
            )
            .expect("a valid timing"),
            block: None,
        }
    }

    pub fn new_schedule(seeded_card: &SeededCard, settings: ScheduleSettings) -> NewCardSchedule {
        NewCardSchedule {
            id: Uuid::now_v7(),
            user_id: seeded_card.user_id,
            card_id: seeded_card.card_id,
            board_id: seeded_card.board_id,
            next_run_at: settings
                .timing
                .next_run_after(instant("2026-11-08T00:00:00Z"))
                .expect("a future run"),
            settings,
        }
    }

    pub async fn seed_schedule(db_pool: &PgPool, new_schedule: &NewCardSchedule) -> Uuid {
        let schedule_write = PostgresCardScheduleSource
            .insert(db_pool, new_schedule)
            .await
            .expect("the insert should run");
        assert_eq!(schedule_write, ScheduleWrite::Written);
        new_schedule.id
    }

    fn expected_schedule(new_schedule: &NewCardSchedule, list_name: &str) -> CardSchedule {
        CardSchedule {
            id: new_schedule.id,
            card_id: new_schedule.card_id,
            board_id: new_schedule.board_id,
            card_title: String::from("Weekly review"),
            target_list_id: Some(new_schedule.settings.target_list_id),
            target_list_name: Some(String::from(list_name)),
            timing: new_schedule.settings.timing.clone(),
            block: new_schedule.settings.block,
            is_paused: false,
            next_run_at: Some(new_schedule.next_run_at),
        }
    }

    #[sqlx::test]
    async fn reads_back_every_field_of_a_schedule(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let category_id = seed_category_id(&db_pool, user_id).await;
        let new_schedule = new_schedule(
            &seeded_card,
            ScheduleSettings {
                block: Some(ScheduleBlock {
                    minutes: 45,
                    category_id: Some(category_id),
                }),
                ..weekly_settings(seeded_card.to_do_list_id)
            },
        );
        let schedule_id = seed_schedule(&db_pool, &new_schedule).await;

        let found_schedule = PostgresCardScheduleSource
            .find_locked(&db_pool, user_id, schedule_id)
            .await
            .expect("the read should run");
        let listed_schedules = PostgresCardScheduleSource
            .list_for_card(&db_pool, seeded_card.card_id)
            .await
            .expect("the list should run");

        let expected_schedule = expected_schedule(&new_schedule, "To Do");
        assert_eq!(found_schedule, Some(expected_schedule.clone()));
        assert_eq!(listed_schedules, vec![expected_schedule]);
        assert_eq!(
            PostgresCardScheduleSource
                .find_card_id(&db_pool, user_id, schedule_id)
                .await
                .expect("the read should run"),
            Some(seeded_card.card_id)
        );
    }

    #[sqlx::test]
    async fn keeps_a_schedule_to_its_owner_and_its_card_s_board(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let other_board_card = seed_card(&db_pool, user_id).await;
        let other_category_id = seed_category_id(&db_pool, other_user_id).await;
        let schedule_id = seed_schedule(
            &db_pool,
            &new_schedule(&seeded_card, weekly_settings(seeded_card.to_do_list_id)),
        )
        .await;

        let other_board_write = PostgresCardScheduleSource
            .insert(
                &db_pool,
                &new_schedule(
                    &seeded_card,
                    weekly_settings(other_board_card.to_do_list_id),
                ),
            )
            .await
            .expect("the insert should run");
        let other_category_write = PostgresCardScheduleSource
            .update(
                &db_pool,
                schedule_id,
                &ScheduleSettings {
                    block: Some(ScheduleBlock {
                        minutes: 30,
                        category_id: Some(other_category_id),
                    }),
                    ..weekly_settings(seeded_card.to_do_list_id)
                },
                instant("2030-01-07T10:00:00Z"),
            )
            .await
            .expect("the update should run");

        assert_eq!(other_board_write, ScheduleWrite::UnknownLink);
        assert_eq!(other_category_write, ScheduleWrite::UnknownLink);
        assert_eq!(
            PostgresCardScheduleSource
                .find_locked(&db_pool, other_user_id, schedule_id)
                .await
                .expect("the read should run"),
            None
        );
        assert_eq!(
            PostgresCardScheduleSource
                .find_card_id(&db_pool, other_user_id, schedule_id)
                .await
                .expect("the read should run"),
            None
        );
    }

    #[sqlx::test]
    async fn changes_pauses_counts_and_deletes_a_schedule(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let new_schedule = new_schedule(&seeded_card, weekly_settings(seeded_card.to_do_list_id));
        let schedule_id = seed_schedule(&db_pool, &new_schedule).await;
        let changed_settings = ScheduleSettings {
            block: Some(ScheduleBlock {
                minutes: 30,
                category_id: None,
            }),
            ..weekly_settings(seeded_card.doing_list_id)
        };

        let update_write = PostgresCardScheduleSource
            .update(
                &db_pool,
                schedule_id,
                &changed_settings,
                instant("2030-01-14T10:00:00Z"),
            )
            .await
            .expect("the update should run");
        PostgresCardScheduleSource
            .set_paused(&db_pool, schedule_id, true, None)
            .await
            .expect("the pause should run");
        let paused_schedule = PostgresCardScheduleSource
            .find_locked(&db_pool, user_id, schedule_id)
            .await
            .expect("the read should run");
        let schedule_count = PostgresCardScheduleSource
            .count_for_card(&db_pool, seeded_card.card_id)
            .await
            .expect("the count should run");
        let is_deleted = PostgresCardScheduleSource
            .delete(&db_pool, schedule_id)
            .await
            .expect("the delete should run");

        assert_eq!(update_write, ScheduleWrite::Written);
        assert_eq!(
            paused_schedule,
            Some(CardSchedule {
                target_list_name: Some(String::from("Doing")),
                block: changed_settings.block,
                is_paused: true,
                next_run_at: None,
                ..expected_schedule(
                    &NewCardSchedule {
                        settings: changed_settings.clone(),
                        ..new_schedule
                    },
                    "Doing"
                )
            })
        );
        assert_eq!(schedule_count, 1);
        assert!(is_deleted);
        assert_eq!(
            PostgresCardScheduleSource
                .count_for_card(&db_pool, seeded_card.card_id)
                .await
                .expect("the count should run"),
            0
        );
    }

    #[sqlx::test]
    async fn lists_the_soonest_active_move_per_card_and_every_active_schedule(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let later_schedule = NewCardSchedule {
            next_run_at: instant("2030-02-04T10:00:00Z"),
            ..new_schedule(&seeded_card, weekly_settings(seeded_card.doing_list_id))
        };
        let sooner_schedule =
            new_schedule(&seeded_card, weekly_settings(seeded_card.to_do_list_id));
        let paused_schedule = NewCardSchedule {
            next_run_at: instant("2029-01-01T10:00:00Z"),
            ..new_schedule(&seeded_card, weekly_settings(seeded_card.doing_list_id))
        };
        for seeded_schedule in [&later_schedule, &sooner_schedule, &paused_schedule] {
            seed_schedule(&db_pool, seeded_schedule).await;
        }
        PostgresCardScheduleSource
            .set_paused(
                &db_pool,
                paused_schedule.id,
                true,
                Some(paused_schedule.next_run_at),
            )
            .await
            .expect("the pause should run");

        let next_moves = PostgresCardScheduleSource
            .list_next_moves(&db_pool, seeded_card.board_id)
            .await
            .expect("the list should run");
        let active_schedule_ids: Vec<Uuid> = PostgresCardScheduleSource
            .list_active(&db_pool, user_id)
            .await
            .expect("the list should run")
            .iter()
            .map(|active_schedule| active_schedule.id)
            .collect();

        assert_eq!(
            next_moves,
            vec![(
                seeded_card.card_id,
                NextMove {
                    at: sooner_schedule.next_run_at,
                    list_name: String::from("To Do"),
                }
            )]
        );
        assert_eq!(
            active_schedule_ids,
            vec![later_schedule.id, sooner_schedule.id]
        );
    }

    #[sqlx::test]
    async fn sets_and_clears_the_next_run(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let schedule_id = seed_schedule(
            &db_pool,
            &new_schedule(&seeded_card, weekly_settings(seeded_card.to_do_list_id)),
        )
        .await;
        let next_run_of = |db_pool: PgPool| async move {
            PostgresCardScheduleSource
                .find_locked(&db_pool, user_id, schedule_id)
                .await
                .expect("the read should run")
                .and_then(|card_schedule| card_schedule.next_run_at)
        };

        PostgresCardScheduleSource
            .set_next_run_at(&db_pool, schedule_id, Some(instant("2030-01-21T10:00:00Z")))
            .await
            .expect("the next run should be set");
        let set_next_run = next_run_of(db_pool.clone()).await;
        PostgresCardScheduleSource
            .set_next_run_at(&db_pool, schedule_id, None)
            .await
            .expect("the next run should be cleared");
        let cleared_next_run = next_run_of(db_pool.clone()).await;

        assert_eq!(set_next_run, Some(instant("2030-01-21T10:00:00Z")));
        assert_eq!(cleared_next_run, None);
    }

    #[sqlx::test]
    async fn lists_due_schedules_oldest_first_without_paused_or_flagged_ones(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let mut schedule_ids = Vec::new();
        for next_run_text in [
            "2030-01-07T10:00:00Z",
            "2030-01-06T10:00:00Z",
            "2030-01-05T10:00:00Z",
            "2030-01-04T10:00:00Z",
            "2030-01-08T10:00:00Z",
        ] {
            let schedule_id = seed_schedule(
                &db_pool,
                &new_schedule(&seeded_card, weekly_settings(seeded_card.to_do_list_id)),
            )
            .await;
            PostgresCardScheduleSource
                .set_next_run_at(&db_pool, schedule_id, Some(instant(next_run_text)))
                .await
                .expect("the next run should be set");
            schedule_ids.push(schedule_id);
        }
        PostgresCardScheduleSource
            .set_paused(
                &db_pool,
                schedule_ids[2],
                true,
                Some(instant("2030-01-05T10:00:00Z")),
            )
            .await
            .expect("the pause should run");
        sqlx::query!(
            "UPDATE card_schedules SET target_list_id = NULL WHERE id = $1",
            schedule_ids[3],
        )
        .execute(&db_pool)
        .await
        .expect("the flag should be set");

        let due_schedules = PostgresCardScheduleSource
            .list_due(&db_pool, instant("2030-01-07T10:00:00Z"), 100)
            .await
            .expect("the read should run");
        let limited_schedules = PostgresCardScheduleSource
            .list_due(&db_pool, instant("2030-01-07T10:00:00Z"), 1)
            .await
            .expect("the read should run");

        let due_schedule_of = |schedule_id| DueSchedule {
            schedule_id,
            user_id,
        };
        assert_eq!(
            due_schedules,
            vec![
                due_schedule_of(schedule_ids[1]),
                due_schedule_of(schedule_ids[0])
            ]
        );
        assert_eq!(limited_schedules, vec![due_schedule_of(schedule_ids[1])]);
    }

    #[sqlx::test]
    async fn pauses_only_the_active_schedules_into_a_list(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let mut schedule_ids = Vec::new();
        for target_list_id in [
            seeded_card.to_do_list_id,
            seeded_card.to_do_list_id,
            seeded_card.doing_list_id,
        ] {
            schedule_ids.push(
                seed_schedule(
                    &db_pool,
                    &new_schedule(&seeded_card, weekly_settings(target_list_id)),
                )
                .await,
            );
        }
        PostgresCardScheduleSource
            .set_paused(&db_pool, schedule_ids[1], true, None)
            .await
            .expect("the pause should run");

        let paused_ids = PostgresCardScheduleSource
            .pause_into_list(&db_pool, seeded_card.to_do_list_id)
            .await
            .expect("the pause should run");

        assert_eq!(paused_ids, vec![schedule_ids[0]]);
        let doing_schedule = PostgresCardScheduleSource
            .find_locked(&db_pool, user_id, schedule_ids[2])
            .await
            .expect("the read should run")
            .expect("the schedule should exist");
        assert!(!doing_schedule.is_paused);
    }

    #[sqlx::test]
    async fn lists_the_schedules_of_a_card_list_or_board(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let schedule_id = seed_schedule(
            &db_pool,
            &new_schedule(&seeded_card, weekly_settings(seeded_card.to_do_list_id)),
        )
        .await;
        let other_card = seed_card(&db_pool, other_user_id).await;
        seed_schedule(
            &db_pool,
            &new_schedule(&other_card, weekly_settings(other_card.to_do_list_id)),
        )
        .await;

        for (deleted_cards, expected_ids) in [
            (DeletedCards::Card(seeded_card.card_id), vec![schedule_id]),
            (
                DeletedCards::BoardList(seeded_card.doing_list_id),
                vec![schedule_id],
            ),
            (
                DeletedCards::BoardList(seeded_card.to_do_list_id),
                Vec::new(),
            ),
            (DeletedCards::Board(seeded_card.board_id), vec![schedule_id]),
            (DeletedCards::Board(other_card.board_id), Vec::new()),
        ] {
            let listed_ids = PostgresCardScheduleSource
                .list_ids_of_deleted_cards(&db_pool, user_id, deleted_cards)
                .await
                .expect("the read should run");
            assert_eq!(listed_ids, expected_ids, "{deleted_cards:?}");
        }
    }
}
