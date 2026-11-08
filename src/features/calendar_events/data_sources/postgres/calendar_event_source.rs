use sqlx::PgExecutor;
use sqlx::types::Json;
use uuid::Uuid;

use crate::core::recurrence::RepeatRule;
use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::entities::calendar_event::{
    CalendarEvent, CalendarEventKind, CalendarEventSource, ChangedCalendarEvent, ClosingScope,
    EventRange, EventWrite, NewCalendarEvent, NewRunningEvent, RunningItem, RunningOpen,
    SeriesOccurrence, TrackedSubject,
};

#[derive(Default)]
pub struct PostgresCalendarEventSource;

struct CalendarEventRow {
    id: Uuid,
    is_tracked: bool,
    title: String,
    category_id: Option<Uuid>,
    task_id: Option<Uuid>,
    task_group_id: Option<Uuid>,
    card_id: Option<Uuid>,
    card_board_id: Option<Uuid>,
    starts_at_millis: i64,
    ends_at_millis: Option<i64>,
    finished_at_millis: Option<i64>,
    last_answered_at_millis: Option<i64>,
    time_zone: Option<String>,
    series_id: Option<Uuid>,
    original_start_millis: Option<i64>,
    series_repeat_rule: Option<Json<RepeatRule>>,
}

impl From<CalendarEventRow> for CalendarEvent {
    fn from(event_row: CalendarEventRow) -> Self {
        let source = match (
            event_row.task_id.zip(event_row.task_group_id),
            event_row.card_id.zip(event_row.card_board_id),
        ) {
            (Some((task_id, group_id)), _) => Some(CalendarEventSource::Task { task_id, group_id }),
            (None, Some((card_id, board_id))) => {
                Some(CalendarEventSource::Card { card_id, board_id })
            }
            (None, None) => None,
        };
        Self {
            id: event_row.id,
            kind: if event_row.is_tracked {
                CalendarEventKind::Tracked
            } else {
                CalendarEventKind::Planned
            },
            title: event_row.title,
            category_id: event_row.category_id,
            source,
            starts_at: UtcInstant::from_unix_millis(event_row.starts_at_millis),
            ends_at: event_row.ends_at_millis.map(UtcInstant::from_unix_millis),
            finished_at: event_row
                .finished_at_millis
                .map(UtcInstant::from_unix_millis),
            last_answered_at: event_row
                .last_answered_at_millis
                .map(UtcInstant::from_unix_millis),
            time_zone: event_row.time_zone,
            series: match (
                event_row.series_id,
                event_row.original_start_millis,
                event_row.series_repeat_rule,
            ) {
                (Some(series_id), Some(original_start_millis), Some(Json(repeat_rule))) => {
                    Some(SeriesOccurrence {
                        series_id,
                        original_start: UtcInstant::from_unix_millis(original_start_millis),
                        repeat_rule,
                    })
                }
                _ => None,
            },
        }
    }
}

struct RunningItemRow {
    id: Uuid,
    title: String,
    category_id: Option<Uuid>,
    task_id: Option<Uuid>,
    task_group_id: Option<Uuid>,
    task_estimate_minutes: Option<i32>,
    card_id: Option<Uuid>,
    card_board_id: Option<Uuid>,
    source_name: Option<String>,
    starts_at_millis: i64,
    last_answered_at_millis: Option<i64>,
}

impl From<RunningItemRow> for RunningItem {
    fn from(item_row: RunningItemRow) -> Self {
        Self {
            estimate_minutes: item_row.task_estimate_minutes,
            source_name: item_row.source_name,
            event: CalendarEvent::from(CalendarEventRow {
                id: item_row.id,
                is_tracked: true,
                title: item_row.title,
                category_id: item_row.category_id,
                task_id: item_row.task_id,
                task_group_id: item_row.task_group_id,
                card_id: item_row.card_id,
                card_board_id: item_row.card_board_id,
                starts_at_millis: item_row.starts_at_millis,
                ends_at_millis: None,
                finished_at_millis: None,
                last_answered_at_millis: item_row.last_answered_at_millis,
                time_zone: None,
                series_id: None,
                original_start_millis: None,
                series_repeat_rule: None,
            }),
        }
    }
}

fn millis_of(instant: Option<UtcInstant>) -> Option<i64> {
    instant.map(UtcInstant::unix_millis)
}

fn starts_millis_of(original_starts: &[UtcInstant]) -> Vec<i64> {
    original_starts
        .iter()
        .map(|original_start| original_start.unix_millis())
        .collect()
}

fn write_outcome(write_result: Result<(), sqlx::Error>) -> Result<EventWrite, sqlx::Error> {
    match write_result {
        Ok(()) => Ok(EventWrite::Written),
        Err(sqlx::Error::Database(database_error)) if database_error.is_foreign_key_violation() => {
            Ok(EventWrite::UnknownLink)
        }
        Err(other_error) => Err(other_error),
    }
}

impl PostgresCalendarEventSource {
    pub async fn list_in_range(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        event_range: EventRange,
    ) -> Result<Vec<CalendarEvent>, sqlx::Error> {
        let event_rows = sqlx::query_as!(
            CalendarEventRow,
            r#"SELECT calendar_events.id,
                      calendar_events.kind = 'tracked' AS "is_tracked!",
                      calendar_events.title,
                      calendar_events.category_id,
                      calendar_events.task_id,
                      tasks.group_id AS "task_group_id?",
                      calendar_events.card_id,
                      cards.board_id AS "card_board_id?",
                      (extract(epoch FROM calendar_events.starts_at) * 1000)::bigint AS "starts_at_millis!",
                      (extract(epoch FROM calendar_events.ends_at) * 1000)::bigint AS ends_at_millis,
                      (extract(epoch FROM calendar_events.finished_at) * 1000)::bigint AS finished_at_millis,
                      (extract(epoch FROM calendar_events.last_answered_at) * 1000)::bigint AS last_answered_at_millis,
                      calendar_events.time_zone,
                      calendar_events.series_id,
                      (extract(epoch FROM calendar_events.original_start) * 1000)::bigint AS original_start_millis,
                      event_series.repeat_rule AS "series_repeat_rule?: Json<RepeatRule>"
               FROM calendar_events
               LEFT JOIN tasks ON tasks.id = calendar_events.task_id
               LEFT JOIN cards ON cards.id = calendar_events.card_id
               LEFT JOIN event_series ON event_series.id = calendar_events.series_id
               WHERE calendar_events.user_id = $1
                 AND calendar_events.starts_at < 'epoch'::timestamptz + $3::bigint * interval '1 millisecond'
                 AND coalesce(calendar_events.ends_at, now())
                     > 'epoch'::timestamptz + $2::bigint * interval '1 millisecond'
               ORDER BY calendar_events.starts_at, calendar_events.id"#,
            user_id,
            event_range.from.unix_millis(),
            event_range.to.unix_millis(),
        )
        .fetch_all(executor)
        .await?;
        Ok(event_rows.into_iter().map(CalendarEvent::from).collect())
    }

    pub async fn find_locked(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        event_id: Uuid,
    ) -> Result<Option<CalendarEvent>, sqlx::Error> {
        let event_row = sqlx::query_as!(
            CalendarEventRow,
            r#"SELECT calendar_events.id,
                      calendar_events.kind = 'tracked' AS "is_tracked!",
                      calendar_events.title,
                      calendar_events.category_id,
                      calendar_events.task_id,
                      tasks.group_id AS "task_group_id?",
                      calendar_events.card_id,
                      cards.board_id AS "card_board_id?",
                      (extract(epoch FROM calendar_events.starts_at) * 1000)::bigint AS "starts_at_millis!",
                      (extract(epoch FROM calendar_events.ends_at) * 1000)::bigint AS ends_at_millis,
                      (extract(epoch FROM calendar_events.finished_at) * 1000)::bigint AS finished_at_millis,
                      (extract(epoch FROM calendar_events.last_answered_at) * 1000)::bigint AS last_answered_at_millis,
                      calendar_events.time_zone,
                      calendar_events.series_id,
                      (extract(epoch FROM calendar_events.original_start) * 1000)::bigint AS original_start_millis,
                      event_series.repeat_rule AS "series_repeat_rule?: Json<RepeatRule>"
               FROM calendar_events
               LEFT JOIN tasks ON tasks.id = calendar_events.task_id
               LEFT JOIN cards ON cards.id = calendar_events.card_id
               LEFT JOIN event_series ON event_series.id = calendar_events.series_id
               WHERE calendar_events.id = $2 AND calendar_events.user_id = $1
               FOR UPDATE OF calendar_events"#,
            user_id,
            event_id,
        )
        .fetch_optional(executor)
        .await?;
        Ok(event_row.map(CalendarEvent::from))
    }

    pub async fn find_override_locked(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        series_id: Uuid,
        original_start: UtcInstant,
    ) -> Result<Option<CalendarEvent>, sqlx::Error> {
        let event_row = sqlx::query_as!(
            CalendarEventRow,
            r#"SELECT calendar_events.id,
                      calendar_events.kind = 'tracked' AS "is_tracked!",
                      calendar_events.title,
                      calendar_events.category_id,
                      calendar_events.task_id,
                      NULL::uuid AS "task_group_id?",
                      calendar_events.card_id,
                      NULL::uuid AS "card_board_id?",
                      (extract(epoch FROM calendar_events.starts_at) * 1000)::bigint AS "starts_at_millis!",
                      (extract(epoch FROM calendar_events.ends_at) * 1000)::bigint AS ends_at_millis,
                      (extract(epoch FROM calendar_events.finished_at) * 1000)::bigint AS finished_at_millis,
                      (extract(epoch FROM calendar_events.last_answered_at) * 1000)::bigint AS last_answered_at_millis,
                      calendar_events.time_zone,
                      calendar_events.series_id,
                      (extract(epoch FROM calendar_events.original_start) * 1000)::bigint AS original_start_millis,
                      event_series.repeat_rule AS "series_repeat_rule?: Json<RepeatRule>"
               FROM calendar_events
               JOIN event_series ON event_series.id = calendar_events.series_id
               WHERE calendar_events.user_id = $1
                 AND calendar_events.series_id = $2
                 AND calendar_events.original_start
                     = 'epoch'::timestamptz + $3::bigint * interval '1 millisecond'
               FOR UPDATE OF calendar_events"#,
            user_id,
            series_id,
            original_start.unix_millis(),
        )
        .fetch_optional(executor)
        .await?;
        Ok(event_row.map(CalendarEvent::from))
    }

    pub async fn list_override_starts(
        &self,
        executor: impl PgExecutor<'_>,
        series_id: Uuid,
    ) -> Result<Vec<UtcInstant>, sqlx::Error> {
        let original_start_millis = sqlx::query_scalar!(
            r#"SELECT (extract(epoch FROM original_start) * 1000)::bigint AS "original_start_millis!"
               FROM calendar_events
               WHERE series_id = $1
               ORDER BY original_start"#,
            series_id,
        )
        .fetch_all(executor)
        .await?;
        Ok(original_start_millis
            .into_iter()
            .map(UtcInstant::from_unix_millis)
            .collect())
    }

    pub async fn delete_overrides(
        &self,
        executor: impl PgExecutor<'_>,
        series_id: Uuid,
        original_starts: &[UtcInstant],
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM calendar_events
             WHERE series_id = $1
               AND original_start = ANY(
                   SELECT 'epoch'::timestamptz + start_millis * interval '1 millisecond'
                   FROM unnest($2::bigint[]) AS start_millis)",
            series_id,
            &starts_millis_of(original_starts),
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn move_overrides(
        &self,
        executor: impl PgExecutor<'_>,
        from_series_id: Uuid,
        to_series_id: Uuid,
        original_starts: &[UtcInstant],
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE calendar_events
             SET series_id = $2, updated_at = now()
             WHERE series_id = $1
               AND original_start = ANY(
                   SELECT 'epoch'::timestamptz + start_millis * interval '1 millisecond'
                   FROM unnest($3::bigint[]) AS start_millis)",
            from_series_id,
            to_series_id,
            &starts_millis_of(original_starts),
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn insert(
        &self,
        executor: impl PgExecutor<'_>,
        new_event: &NewCalendarEvent,
    ) -> Result<EventWrite, sqlx::Error> {
        let insert_result = sqlx::query!(
            "INSERT INTO calendar_events
                 (id, user_id, kind, title, category_id, task_id, card_id,
                  starts_at, ends_at, finished_at, last_answered_at,
                  time_zone, series_id, original_start)
             VALUES ($1, $2, $3, $4, $5, $6, $7,
                     'epoch'::timestamptz + $8::bigint * interval '1 millisecond',
                     'epoch'::timestamptz + $9::bigint * interval '1 millisecond',
                     'epoch'::timestamptz + $10::bigint * interval '1 millisecond',
                     'epoch'::timestamptz + $11::bigint * interval '1 millisecond',
                     $12, $13,
                     'epoch'::timestamptz + $14::bigint * interval '1 millisecond')",
            new_event.id,
            new_event.user_id,
            new_event.kind.as_str(),
            new_event.title,
            new_event.category_id,
            new_event.task_id,
            new_event.card_id,
            new_event.starts_at.unix_millis(),
            millis_of(new_event.ends_at),
            millis_of(new_event.finished_at),
            millis_of(new_event.last_answered_at),
            new_event.time_zone.as_deref(),
            new_event.series_id,
            millis_of(new_event.original_start),
        )
        .execute(executor)
        .await
        .map(|_| ());
        write_outcome(insert_result)
    }

    pub async fn update(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        event_id: Uuid,
        changed_event: &ChangedCalendarEvent,
    ) -> Result<EventWrite, sqlx::Error> {
        let update_result = sqlx::query!(
            "UPDATE calendar_events
             SET title = $3,
                 category_id = $4,
                 starts_at = 'epoch'::timestamptz + $5::bigint * interval '1 millisecond',
                 ends_at = 'epoch'::timestamptz + $6::bigint * interval '1 millisecond',
                 finished_at = 'epoch'::timestamptz + $7::bigint * interval '1 millisecond',
                 time_zone = $8,
                 updated_at = now()
             WHERE id = $2 AND user_id = $1",
            user_id,
            event_id,
            changed_event.title,
            changed_event.category_id,
            changed_event.starts_at.unix_millis(),
            millis_of(changed_event.ends_at),
            millis_of(changed_event.finished_at),
            changed_event.time_zone.as_deref(),
        )
        .execute(executor)
        .await
        .map(|_| ());
        write_outcome(update_result)
    }

    pub async fn find_task_id(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        event_id: Uuid,
    ) -> Result<Option<Option<Uuid>>, sqlx::Error> {
        sqlx::query_scalar!(
            "SELECT task_id FROM calendar_events WHERE id = $2 AND user_id = $1",
            user_id,
            event_id,
        )
        .fetch_optional(executor)
        .await
    }

    pub async fn open_running(
        &self,
        executor: impl PgExecutor<'_>,
        new_running_event: &NewRunningEvent,
    ) -> Result<RunningOpen, sqlx::Error> {
        let (task_id, card_id) = match new_running_event.subject {
            TrackedSubject::Task(task_id) => (Some(task_id), None),
            TrackedSubject::Card(card_id) => (None, Some(card_id)),
        };
        let insert_result = sqlx::query!(
            "INSERT INTO calendar_events
                 (id, user_id, kind, title, category_id, task_id, card_id, starts_at)
             SELECT $1, users.id, 'tracked', $3, users.tracked_category_id, $4, $5, now()
             FROM users
             WHERE users.id = $2",
            new_running_event.id,
            new_running_event.user_id,
            new_running_event.title,
            task_id,
            card_id,
        )
        .execute(executor)
        .await;
        match insert_result {
            Ok(_) => Ok(RunningOpen::Opened),
            Err(sqlx::Error::Database(database_error)) if database_error.is_unique_violation() => {
                Ok(RunningOpen::AlreadyRunning)
            }
            Err(other_error) => Err(other_error),
        }
    }

    pub async fn list_running(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
    ) -> Result<Vec<RunningItem>, sqlx::Error> {
        let item_rows = sqlx::query_as!(
            RunningItemRow,
            r#"SELECT calendar_events.id AS "id!",
                      calendar_events.title AS "title!",
                      calendar_events.category_id,
                      calendar_events.task_id,
                      tasks.group_id AS "task_group_id?",
                      tasks.estimate_minutes AS "task_estimate_minutes?",
                      calendar_events.card_id,
                      cards.board_id AS "card_board_id?",
                      coalesce(task_groups.name, boards.name) AS source_name,
                      (extract(epoch FROM calendar_events.starts_at) * 1000)::bigint AS "starts_at_millis!",
                      (extract(epoch FROM calendar_events.last_answered_at) * 1000)::bigint AS last_answered_at_millis
               FROM calendar_events
               LEFT JOIN tasks ON tasks.id = calendar_events.task_id
               LEFT JOIN task_groups ON task_groups.id = tasks.group_id
               LEFT JOIN cards ON cards.id = calendar_events.card_id
               LEFT JOIN boards ON boards.id = cards.board_id
               WHERE calendar_events.user_id = $1 AND calendar_events.ends_at IS NULL
               ORDER BY calendar_events.starts_at DESC, calendar_events.id DESC"#,
            user_id,
        )
        .fetch_all(executor)
        .await?;
        Ok(item_rows.into_iter().map(RunningItem::from).collect())
    }

    pub async fn find_running_item(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        event_id: Uuid,
    ) -> Result<Option<RunningItem>, sqlx::Error> {
        let item_row = sqlx::query_as!(
            RunningItemRow,
            r#"SELECT calendar_events.id AS "id!",
                      calendar_events.title AS "title!",
                      calendar_events.category_id,
                      calendar_events.task_id,
                      tasks.group_id AS "task_group_id?",
                      tasks.estimate_minutes AS "task_estimate_minutes?",
                      calendar_events.card_id,
                      cards.board_id AS "card_board_id?",
                      coalesce(task_groups.name, boards.name) AS source_name,
                      (extract(epoch FROM calendar_events.starts_at) * 1000)::bigint AS "starts_at_millis!",
                      (extract(epoch FROM calendar_events.last_answered_at) * 1000)::bigint AS last_answered_at_millis
               FROM calendar_events
               LEFT JOIN tasks ON tasks.id = calendar_events.task_id
               LEFT JOIN task_groups ON task_groups.id = tasks.group_id
               LEFT JOIN cards ON cards.id = calendar_events.card_id
               LEFT JOIN boards ON boards.id = cards.board_id
               WHERE calendar_events.id = $2
                 AND calendar_events.user_id = $1
                 AND calendar_events.ends_at IS NULL"#,
            user_id,
            event_id,
        )
        .fetch_optional(executor)
        .await?;
        Ok(item_row.map(RunningItem::from))
    }

    pub async fn close_running(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        closing_scope: ClosingScope,
    ) -> Result<u64, sqlx::Error> {
        let (task_id, task_group_id, card_id, list_id, board_id) = match closing_scope {
            ClosingScope::Task(task_id) => (Some(task_id), None, None, None, None),
            ClosingScope::TaskGroup(task_group_id) => (None, Some(task_group_id), None, None, None),
            ClosingScope::Card(card_id) => (None, None, Some(card_id), None, None),
            ClosingScope::BoardList(list_id) => (None, None, None, Some(list_id), None),
            ClosingScope::Board(board_id) => (None, None, None, None, Some(board_id)),
        };
        let close_result = sqlx::query!(
            "UPDATE calendar_events
             SET ends_at = now(), finished_at = now(), updated_at = now()
             WHERE calendar_events.user_id = $1
               AND calendar_events.ends_at IS NULL
               AND (
                   calendar_events.task_id = $2
                   OR calendar_events.card_id = $4
                   OR EXISTS (
                       SELECT FROM tasks
                       WHERE tasks.id = calendar_events.task_id AND tasks.group_id = $3
                   )
                   OR EXISTS (
                       SELECT FROM cards
                       WHERE cards.id = calendar_events.card_id
                         AND (cards.list_id = $5 OR cards.board_id = $6)
                   )
               )",
            user_id,
            task_id,
            task_group_id,
            card_id,
            list_id,
            board_id,
        )
        .execute(executor)
        .await?;
        Ok(close_result.rows_affected())
    }

    pub async fn rename_tracked(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        tracked_subject: TrackedSubject,
        title: &str,
    ) -> Result<u64, sqlx::Error> {
        let (task_id, card_id) = match tracked_subject {
            TrackedSubject::Task(task_id) => (Some(task_id), None),
            TrackedSubject::Card(card_id) => (None, Some(card_id)),
        };
        let rename_result = sqlx::query!(
            "UPDATE calendar_events
             SET title = $4, updated_at = now()
             WHERE calendar_events.user_id = $1
               AND (calendar_events.task_id = $2 OR calendar_events.card_id = $3)",
            user_id,
            task_id,
            card_id,
            title,
        )
        .execute(executor)
        .await?;
        Ok(rename_result.rows_affected())
    }

    pub async fn record_check_in(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        event_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE calendar_events
             SET last_answered_at = now(), updated_at = now()
             WHERE id = $2 AND user_id = $1 AND ends_at IS NULL",
            user_id,
            event_id,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn delete(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        event_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let delete_result = sqlx::query!(
            "DELETE FROM calendar_events WHERE id = $2 AND user_id = $1",
            user_id,
            event_id,
        )
        .execute(executor)
        .await?;
        Ok(delete_result.rows_affected() == 1)
    }
}

#[cfg(test)]
pub mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::board_lists::actions::create_board_list::CreateBoardList;
    use crate::features::boards::actions::create_board::CreateBoard;
    use crate::features::cards::actions::create_card::CreateCard;
    use crate::features::categories::actions::create_category::CreateCategory;
    use crate::features::task_groups::actions::create_task_group::CreateTaskGroup;
    use crate::features::tasks::actions::add_task::AddTask;
    use crate::features::users::actions::create_user::CreateUser;
    use crate::features::users::entities::user::NewUser;

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

    pub async fn seed_task_ids(db_pool: &PgPool, user_id: Uuid) -> (Uuid, Uuid) {
        let task_group = CreateTaskGroup::new()
            .execute(db_pool, user_id, "DaiBX")
            .await
            .expect("the group should be created");
        let group_tasks = AddTask::new(db_pool)
            .execute(user_id, task_group.id, "Write the changelog", None)
            .await
            .expect("the task should be added");
        let task_id = group_tasks
            .tasks
            .first()
            .expect("the added task should be listed")
            .id;
        (task_id, task_group.id)
    }

    pub async fn seed_card_in_list(db_pool: &PgPool, user_id: Uuid) -> (Uuid, Uuid, Uuid) {
        let board = CreateBoard::new()
            .execute(db_pool, user_id, "Projects")
            .await
            .expect("the board should be created");
        let board = CreateBoardList::new(db_pool)
            .execute(user_id, board.board.id, "Doing")
            .await
            .expect("the list should be created");
        let list_id = board.lists.first().expect("the list should be listed").id;
        let board = CreateCard::new(db_pool)
            .execute(user_id, list_id, "Plan the release")
            .await
            .expect("the card should be created");
        let card_id = board.cards.first().expect("the card should be listed").id;
        (card_id, list_id, board.board.id)
    }

    pub async fn seed_card_ids(db_pool: &PgPool, user_id: Uuid) -> (Uuid, Uuid) {
        let (card_id, _, board_id) = seed_card_in_list(db_pool, user_id).await;
        (card_id, board_id)
    }

    pub fn new_event(user_id: Uuid, kind: CalendarEventKind) -> NewCalendarEvent {
        NewCalendarEvent {
            id: Uuid::now_v7(),
            user_id,
            kind,
            title: String::from("Focus block"),
            category_id: None,
            task_id: None,
            card_id: None,
            starts_at: instant("2026-11-05T09:00:00Z"),
            ends_at: match kind {
                CalendarEventKind::Planned => Some(instant("2026-11-05T10:00:00Z")),
                CalendarEventKind::Tracked => None,
            },
            finished_at: None,
            last_answered_at: None,
            time_zone: None,
            series_id: None,
            original_start: None,
        }
    }

    pub async fn seed_event(db_pool: &PgPool, new_event: &NewCalendarEvent) -> Uuid {
        let event_write = PostgresCalendarEventSource
            .insert(db_pool, new_event)
            .await
            .expect("the insert should run");
        assert_eq!(event_write, EventWrite::Written);
        new_event.id
    }

    pub async fn find_event(db_pool: &PgPool, user_id: Uuid, event_id: Uuid) -> CalendarEvent {
        PostgresCalendarEventSource
            .find_locked(db_pool, user_id, event_id)
            .await
            .expect("the read should run")
            .expect("the event should exist")
    }

    fn week_range() -> EventRange {
        EventRange {
            from: instant("2026-11-02T00:00:00Z"),
            to: instant("2026-11-09T00:00:00Z"),
        }
    }

    #[sqlx::test]
    async fn reads_back_every_field_of_a_finished_task_event(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let category_id = seed_category_id(&db_pool, user_id).await;
        let (task_id, group_id) = seed_task_ids(&db_pool, user_id).await;
        let event_id = seed_event(
            &db_pool,
            &NewCalendarEvent {
                category_id: Some(category_id),
                task_id: Some(task_id),
                ends_at: Some(instant("2026-11-05T09:40:00.123Z")),
                finished_at: Some(instant("2026-11-05T10:00:00Z")),
                last_answered_at: Some(instant("2026-11-05T09:40:00.123Z")),
                ..new_event(user_id, CalendarEventKind::Tracked)
            },
        )
        .await;

        let stored_event = find_event(&db_pool, user_id, event_id).await;

        assert_eq!(
            stored_event,
            CalendarEvent {
                id: event_id,
                kind: CalendarEventKind::Tracked,
                title: String::from("Focus block"),
                category_id: Some(category_id),
                source: Some(CalendarEventSource::Task { task_id, group_id }),
                starts_at: instant("2026-11-05T09:00:00Z"),
                ends_at: Some(instant("2026-11-05T09:40:00.123Z")),
                finished_at: Some(instant("2026-11-05T10:00:00Z")),
                last_answered_at: Some(instant("2026-11-05T09:40:00.123Z")),
                time_zone: None,
                series: None,
            }
        );
    }

    #[sqlx::test]
    async fn reads_a_card_event_s_board(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let (card_id, board_id) = seed_card_ids(&db_pool, user_id).await;
        let event_id = seed_event(
            &db_pool,
            &NewCalendarEvent {
                card_id: Some(card_id),
                ..new_event(user_id, CalendarEventKind::Tracked)
            },
        )
        .await;

        let stored_event = find_event(&db_pool, user_id, event_id).await;

        assert_eq!(
            stored_event.source,
            Some(CalendarEventSource::Card { card_id, board_id })
        );
        assert!(stored_event.is_running());
    }

    #[sqlx::test]
    async fn lists_events_overlapping_the_range_including_running_ones(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        for (title, starts_at, ends_at, owner_id) in [
            (
                "Before",
                "2026-11-01T08:00:00Z",
                Some("2026-11-02T00:00:00Z"),
                user_id,
            ),
            ("Running", "2026-11-04T08:00:00Z", None, user_id),
            (
                "Inside",
                "2026-11-03T08:00:00Z",
                Some("2026-11-03T09:00:00Z"),
                user_id,
            ),
            (
                "After",
                "2026-11-09T00:00:00Z",
                Some("2026-11-09T01:00:00Z"),
                user_id,
            ),
            (
                "Theirs",
                "2026-11-03T08:00:00Z",
                Some("2026-11-03T09:00:00Z"),
                other_user_id,
            ),
        ] {
            let kind = if ends_at.is_some() {
                CalendarEventKind::Planned
            } else {
                CalendarEventKind::Tracked
            };
            seed_event(
                &db_pool,
                &NewCalendarEvent {
                    title: title.to_owned(),
                    starts_at: instant(starts_at),
                    ends_at: ends_at.map(instant),
                    ..new_event(owner_id, kind)
                },
            )
            .await;
        }

        let listed_events = PostgresCalendarEventSource
            .list_in_range(&db_pool, user_id, week_range())
            .await
            .expect("the list should run");

        let listed_titles: Vec<&str> = listed_events
            .iter()
            .map(|listed_event| listed_event.title.as_str())
            .collect();
        assert_eq!(listed_titles, vec!["Inside", "Running"]);
    }

    #[sqlx::test]
    async fn reports_another_person_s_category_as_an_unknown_link(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let other_category_id = seed_category_id(&db_pool, other_user_id).await;

        let event_write = PostgresCalendarEventSource
            .insert(
                &db_pool,
                &NewCalendarEvent {
                    category_id: Some(other_category_id),
                    ..new_event(user_id, CalendarEventKind::Planned)
                },
            )
            .await
            .expect("the insert should run");

        assert_eq!(event_write, EventWrite::UnknownLink);
    }

    #[sqlx::test]
    async fn updates_the_changed_fields(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let category_id = seed_category_id(&db_pool, user_id).await;
        let event_id = seed_event(&db_pool, &new_event(user_id, CalendarEventKind::Planned)).await;

        let event_write = PostgresCalendarEventSource
            .update(
                &db_pool,
                user_id,
                event_id,
                &ChangedCalendarEvent {
                    title: String::from("Swim"),
                    category_id: Some(category_id),
                    starts_at: instant("2026-11-06T18:00:00Z"),
                    ends_at: Some(instant("2026-11-07T19:00:00Z")),
                    finished_at: None,
                    time_zone: Some(String::from("Europe/London")),
                },
            )
            .await
            .expect("the update should run");

        assert_eq!(event_write, EventWrite::Written);
        let updated_event = find_event(&db_pool, user_id, event_id).await;
        assert_eq!(updated_event.title, "Swim");
        assert_eq!(updated_event.category_id, Some(category_id));
        assert_eq!(updated_event.starts_at, instant("2026-11-06T18:00:00Z"));
        assert_eq!(updated_event.ends_at, Some(instant("2026-11-07T19:00:00Z")));
        assert_eq!(updated_event.time_zone.as_deref(), Some("Europe/London"));
    }

    pub fn new_running_event(
        user_id: Uuid,
        title: &str,
        subject: TrackedSubject,
    ) -> NewRunningEvent {
        NewRunningEvent {
            id: Uuid::now_v7(),
            user_id,
            title: title.to_owned(),
            subject,
        }
    }

    async fn open_running(db_pool: &PgPool, new_running_event: &NewRunningEvent) -> RunningOpen {
        PostgresCalendarEventSource
            .open_running(db_pool, new_running_event)
            .await
            .expect("the insert should run")
    }

    #[sqlx::test]
    async fn opens_a_running_event_for_a_task_and_a_card(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let (task_id, group_id) = seed_task_ids(&db_pool, user_id).await;
        let (card_id, board_id) = seed_card_ids(&db_pool, user_id).await;
        let task_event = new_running_event(
            user_id,
            "Write the changelog",
            TrackedSubject::Task(task_id),
        );
        let card_event =
            new_running_event(user_id, "Plan the release", TrackedSubject::Card(card_id));

        assert_eq!(
            open_running(&db_pool, &task_event).await,
            RunningOpen::Opened
        );
        assert_eq!(
            open_running(&db_pool, &card_event).await,
            RunningOpen::Opened
        );

        let task_item = PostgresCalendarEventSource
            .find_running_item(&db_pool, user_id, task_event.id)
            .await
            .expect("the read should run")
            .expect("the task event should be running");
        assert_eq!(task_item.event.kind, CalendarEventKind::Tracked);
        assert_eq!(task_item.event.title, "Write the changelog");
        assert_eq!(task_item.event.category_id, None);
        assert_eq!(
            task_item.event.source,
            Some(CalendarEventSource::Task { task_id, group_id })
        );
        assert_eq!(task_item.event.ends_at, None);
        assert_eq!(task_item.estimate_minutes, None);
        let card_event_stored = find_event(&db_pool, user_id, card_event.id).await;
        assert_eq!(
            card_event_stored.source,
            Some(CalendarEventSource::Card { card_id, board_id })
        );
        assert!(card_event_stored.is_running());
    }

    #[sqlx::test]
    async fn reports_a_second_running_event_for_the_same_task_or_card(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let (task_id, _) = seed_task_ids(&db_pool, user_id).await;
        let (card_id, _) = seed_card_ids(&db_pool, user_id).await;

        for subject in [TrackedSubject::Task(task_id), TrackedSubject::Card(card_id)] {
            let first_open =
                open_running(&db_pool, &new_running_event(user_id, "First", subject)).await;
            let second_open =
                open_running(&db_pool, &new_running_event(user_id, "Second", subject)).await;
            assert_eq!(first_open, RunningOpen::Opened);
            assert_eq!(second_open, RunningOpen::AlreadyRunning);
        }
    }

    #[sqlx::test]
    async fn lists_only_the_owner_s_running_items_newest_first(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let (task_id, _) = seed_task_ids(&db_pool, user_id).await;
        let (card_id, _) = seed_card_ids(&db_pool, user_id).await;
        let (their_task_id, _) = seed_task_ids(&db_pool, other_user_id).await;
        seed_event(
            &db_pool,
            &NewCalendarEvent {
                title: String::from("Finished"),
                ends_at: Some(instant("2026-11-05T09:30:00Z")),
                finished_at: Some(instant("2026-11-05T09:30:00Z")),
                ..new_event(user_id, CalendarEventKind::Tracked)
            },
        )
        .await;
        seed_event(&db_pool, &new_event(user_id, CalendarEventKind::Planned)).await;
        open_running(
            &db_pool,
            &new_running_event(user_id, "Older", TrackedSubject::Task(task_id)),
        )
        .await;
        open_running(
            &db_pool,
            &new_running_event(user_id, "Newer", TrackedSubject::Card(card_id)),
        )
        .await;
        open_running(
            &db_pool,
            &new_running_event(other_user_id, "Theirs", TrackedSubject::Task(their_task_id)),
        )
        .await;

        let running_items = PostgresCalendarEventSource
            .list_running(&db_pool, user_id)
            .await
            .expect("the list should run");

        let running_titles: Vec<&str> = running_items
            .iter()
            .map(|running_item| running_item.event.title.as_str())
            .collect();
        assert_eq!(running_titles, vec!["Newer", "Older"]);
    }

    #[sqlx::test]
    async fn records_a_check_in_only_on_a_running_event(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let (task_id, _) = seed_task_ids(&db_pool, user_id).await;
        let running_event = new_running_event(user_id, "Running", TrackedSubject::Task(task_id));
        open_running(&db_pool, &running_event).await;
        let planned_event_id =
            seed_event(&db_pool, &new_event(user_id, CalendarEventKind::Planned)).await;

        for event_id in [running_event.id, planned_event_id] {
            PostgresCalendarEventSource
                .record_check_in(&db_pool, user_id, event_id)
                .await
                .expect("the update should run");
        }

        let running_event_stored = find_event(&db_pool, user_id, running_event.id).await;
        let planned_event_stored = find_event(&db_pool, user_id, planned_event_id).await;
        let last_answered_at = running_event_stored
            .last_answered_at
            .expect("the check-in should be recorded");
        assert!(last_answered_at >= running_event_stored.starts_at);
        assert_eq!(planned_event_stored.last_answered_at, None);
    }

    #[sqlx::test]
    async fn reads_the_task_of_only_the_owner_s_event(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let (task_id, _) = seed_task_ids(&db_pool, user_id).await;
        let task_event = new_running_event(user_id, "Task", TrackedSubject::Task(task_id));
        open_running(&db_pool, &task_event).await;
        let planned_event_id =
            seed_event(&db_pool, &new_event(user_id, CalendarEventKind::Planned)).await;
        let calendar_event_source = PostgresCalendarEventSource;

        let task_event_task_id = calendar_event_source
            .find_task_id(&db_pool, user_id, task_event.id)
            .await
            .expect("the read should run");
        let planned_event_task_id = calendar_event_source
            .find_task_id(&db_pool, user_id, planned_event_id)
            .await
            .expect("the read should run");
        let foreign_task_id = calendar_event_source
            .find_task_id(&db_pool, other_user_id, task_event.id)
            .await
            .expect("the read should run");

        assert_eq!(task_event_task_id, Some(Some(task_id)));
        assert_eq!(planned_event_task_id, Some(None));
        assert_eq!(foreign_task_id, None);
    }

    #[sqlx::test]
    async fn closes_only_the_running_events_inside_the_scope(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let (task_id, task_group_id) = seed_task_ids(&db_pool, user_id).await;
        let (card_id, list_id, board_id) = seed_card_in_list(&db_pool, user_id).await;
        let calendar_event_source = PostgresCalendarEventSource;

        for (closing_scope, closed_subject) in [
            (ClosingScope::Task(task_id), TrackedSubject::Task(task_id)),
            (
                ClosingScope::TaskGroup(task_group_id),
                TrackedSubject::Task(task_id),
            ),
            (ClosingScope::Card(card_id), TrackedSubject::Card(card_id)),
            (
                ClosingScope::BoardList(list_id),
                TrackedSubject::Card(card_id),
            ),
            (ClosingScope::Board(board_id), TrackedSubject::Card(card_id)),
        ] {
            let closing_event = new_running_event(user_id, "Inside", closed_subject);
            open_running(&db_pool, &closing_event).await;
            let other_subject = match closed_subject {
                TrackedSubject::Task(_) => TrackedSubject::Card(card_id),
                TrackedSubject::Card(_) => TrackedSubject::Task(task_id),
            };
            let other_event = new_running_event(user_id, "Outside", other_subject);
            open_running(&db_pool, &other_event).await;

            let closed_count = calendar_event_source
                .close_running(&db_pool, user_id, closing_scope)
                .await
                .expect("the close should run");

            assert_eq!(closed_count, 1, "{closing_scope:?}");
            let closed_event = find_event(&db_pool, user_id, closing_event.id).await;
            assert_eq!(closed_event.title, "Inside");
            assert!(closed_event.ends_at.is_some());
            assert_eq!(closed_event.ends_at, closed_event.finished_at);
            assert!(
                find_event(&db_pool, user_id, other_event.id)
                    .await
                    .is_running()
            );
            calendar_event_source
                .close_running(&db_pool, user_id, ClosingScope::Task(task_id))
                .await
                .expect("the close should run");
            calendar_event_source
                .close_running(&db_pool, user_id, ClosingScope::Card(card_id))
                .await
                .expect("the close should run");
        }
    }

    #[sqlx::test]
    async fn closes_nothing_of_another_person(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let (task_id, _) = seed_task_ids(&db_pool, user_id).await;
        let running_event = new_running_event(user_id, "Mine", TrackedSubject::Task(task_id));
        open_running(&db_pool, &running_event).await;

        let closed_count = PostgresCalendarEventSource
            .close_running(&db_pool, other_user_id, ClosingScope::Task(task_id))
            .await
            .expect("the close should run");

        assert_eq!(closed_count, 0);
        assert!(
            find_event(&db_pool, user_id, running_event.id)
                .await
                .is_running()
        );
    }

    #[sqlx::test]
    async fn deletes_only_the_owner_s_event(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let event_id = seed_event(&db_pool, &new_event(user_id, CalendarEventKind::Planned)).await;

        let deleted_by_someone_else = PostgresCalendarEventSource
            .delete(&db_pool, other_user_id, event_id)
            .await
            .expect("the delete should run");
        let deleted_by_owner = PostgresCalendarEventSource
            .delete(&db_pool, user_id, event_id)
            .await
            .expect("the delete should run");

        assert!(!deleted_by_someone_else);
        assert!(deleted_by_owner);
    }

    #[sqlx::test]
    async fn renames_only_the_owner_s_events_of_one_subject(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let (task_id, _) = seed_task_ids(&db_pool, user_id).await;
        let (card_id, _) = seed_card_ids(&db_pool, user_id).await;
        let task_event = new_running_event(
            user_id,
            "Write the changelog",
            TrackedSubject::Task(task_id),
        );
        let card_event =
            new_running_event(user_id, "Plan the release", TrackedSubject::Card(card_id));
        open_running(&db_pool, &task_event).await;
        open_running(&db_pool, &card_event).await;

        let renamed_by_someone_else = PostgresCalendarEventSource
            .rename_tracked(
                &db_pool,
                other_user_id,
                TrackedSubject::Task(task_id),
                "Taken over",
            )
            .await
            .expect("the rename should run");
        let renamed_by_owner = PostgresCalendarEventSource
            .rename_tracked(
                &db_pool,
                user_id,
                TrackedSubject::Task(task_id),
                "Write the release notes",
            )
            .await
            .expect("the rename should run");

        assert_eq!(renamed_by_someone_else, 0);
        assert_eq!(renamed_by_owner, 1);
        assert_eq!(
            find_event(&db_pool, user_id, task_event.id).await.title,
            "Write the release notes"
        );
        assert_eq!(
            find_event(&db_pool, user_id, card_event.id).await.title,
            "Plan the release"
        );
    }
}
