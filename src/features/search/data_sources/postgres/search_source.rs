use jiff::civil::DateTime;
use sqlx::PgExecutor;
use sqlx::types::Json;
use uuid::Uuid;

use crate::core::recurrence::RepeatRule;
use crate::core::utc_instant::UtcInstant;
use crate::features::calendar_events::entities::event_series::EventSeries;
use crate::features::search::entities::search_results::{
    CardMatch, EventMatch, GroupMatch, TaskMatch,
};

#[derive(Default)]
pub struct PostgresSearchSource;

pub struct MatchedSeries {
    pub series: EventSeries,
    pub category_color: Option<String>,
    pub match_rank: f32,
}

struct SingleEventRow {
    id: Uuid,
    title: String,
    category_color: Option<String>,
    starts_at_millis: i64,
    ends_at_millis: i64,
    time_zone: Option<String>,
    linked_task_id: Option<Uuid>,
    match_rank: f32,
}

struct SeriesRow {
    id: Uuid,
    title: String,
    category_id: Option<Uuid>,
    category_color: Option<String>,
    time_zone: String,
    local_start_text: String,
    duration_minutes: i32,
    repeat_rule: Json<RepeatRule>,
    until_instant_millis: Option<i64>,
    cancelled_start_millis: Vec<i64>,
    is_schedule_block: bool,
    match_rank: f32,
}

impl TryFrom<SeriesRow> for MatchedSeries {
    type Error = sqlx::Error;

    fn try_from(series_row: SeriesRow) -> Result<Self, Self::Error> {
        let local_start = series_row
            .local_start_text
            .parse::<DateTime>()
            .map_err(|parse_error| sqlx::Error::Decode(Box::new(parse_error)))?;
        Ok(Self {
            series: EventSeries {
                id: series_row.id,
                title: series_row.title,
                category_id: series_row.category_id,
                time_zone: series_row.time_zone,
                local_start,
                duration_minutes: series_row.duration_minutes,
                repeat_rule: series_row.repeat_rule.0,
                until_instant: series_row
                    .until_instant_millis
                    .map(UtcInstant::from_unix_millis),
                cancelled_starts: series_row
                    .cancelled_start_millis
                    .into_iter()
                    .map(UtcInstant::from_unix_millis)
                    .collect(),
                is_schedule_block: series_row.is_schedule_block,
            },
            category_color: series_row.category_color,
            match_rank: series_row.match_rank,
        })
    }
}

impl PostgresSearchSource {
    pub async fn search_tasks(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        search_words: &[String],
        fetch_limit: i64,
    ) -> Result<Vec<TaskMatch>, sqlx::Error> {
        sqlx::query_as!(
            TaskMatch,
            r#"WITH matched_tasks AS (
                   SELECT tasks.id,
                          tasks.name,
                          tasks.group_id,
                          tasks.parent_id,
                          tasks.completed_at IS NOT NULL AS is_done,
                          ts_rank_cd(tasks.search_vector, search_prefix_query($2)) AS match_rank,
                          tasks.updated_at
                   FROM tasks
                   WHERE tasks.user_id = $1
                     AND tasks.search_vector @@ search_prefix_query($2)
                   ORDER BY is_done, match_rank DESC, tasks.updated_at DESC, tasks.id
                   LIMIT $3
               )
               SELECT matched_tasks.id AS "id!",
                      matched_tasks.name AS "name!",
                      matched_tasks.group_id AS "group_id!",
                      task_groups.name AS "group_name!",
                      coalesce(
                          (WITH RECURSIVE parent_tasks AS (
                               SELECT tasks.id, tasks.parent_id, tasks.name, 1 AS parent_depth
                               FROM tasks
                               WHERE tasks.id = matched_tasks.parent_id AND tasks.user_id = $1
                               UNION ALL
                               SELECT tasks.id, tasks.parent_id, tasks.name, parent_tasks.parent_depth + 1
                               FROM tasks
                               JOIN parent_tasks ON tasks.id = parent_tasks.parent_id
                               WHERE tasks.user_id = $1
                           )
                           SELECT array_agg(parent_tasks.name ORDER BY parent_tasks.parent_depth DESC)
                           FROM parent_tasks),
                          '{}'
                      ) AS "parent_names!",
                      matched_tasks.is_done AS "is_done!",
                      EXISTS (
                          SELECT 1 FROM calendar_events
                          WHERE calendar_events.user_id = $1
                            AND calendar_events.task_id = matched_tasks.id
                            AND calendar_events.kind = 'tracked'
                            AND calendar_events.ends_at IS NULL
                      ) AS "is_running!"
               FROM matched_tasks
               JOIN task_groups
                 ON task_groups.id = matched_tasks.group_id AND task_groups.user_id = $1
               ORDER BY matched_tasks.is_done,
                        matched_tasks.match_rank DESC,
                        matched_tasks.updated_at DESC,
                        matched_tasks.id"#,
            user_id,
            search_words,
            fetch_limit,
        )
        .fetch_all(executor)
        .await
    }

    pub async fn search_groups(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        search_words: &[String],
        fetch_limit: i64,
    ) -> Result<Vec<GroupMatch>, sqlx::Error> {
        sqlx::query_as!(
            GroupMatch,
            r#"SELECT task_groups.id,
                      task_groups.name,
                      (SELECT count(*) FROM tasks
                       WHERE tasks.group_id = task_groups.id
                         AND tasks.user_id = $1
                         AND tasks.completed_at IS NULL) AS "open_task_count!"
               FROM task_groups
               WHERE task_groups.user_id = $1
                 AND task_groups.search_vector @@ search_prefix_query($2)
               ORDER BY ts_rank_cd(task_groups.search_vector, search_prefix_query($2)) DESC,
                        task_groups.updated_at DESC,
                        task_groups.id
               LIMIT $3"#,
            user_id,
            search_words,
            fetch_limit,
        )
        .fetch_all(executor)
        .await
    }

    pub async fn search_cards(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        search_words: &[String],
        fetch_limit: i64,
    ) -> Result<Vec<CardMatch>, sqlx::Error> {
        sqlx::query_as!(
            CardMatch,
            r#"WITH matched_cards AS (
                   SELECT cards.id,
                          cards.title,
                          cards.notes,
                          cards.board_id,
                          cards.list_id,
                          cards.archived_from_list_name,
                          cards.archived_at IS NOT NULL AS is_archived,
                          ts_rank_cd(cards.search_vector, search_prefix_query($2)) AS match_rank,
                          cards.updated_at
                   FROM cards
                   WHERE cards.user_id = $1
                     AND cards.search_vector @@ search_prefix_query($2)
                   ORDER BY is_archived, match_rank DESC, cards.updated_at DESC, cards.id
                   LIMIT $3
               )
               SELECT matched_cards.id AS "id!",
                      matched_cards.title AS "title!",
                      matched_cards.board_id AS "board_id!",
                      boards.name AS "board_name!",
                      coalesce(
                          board_lists.name,
                          matched_cards.archived_from_list_name,
                          ''
                      ) AS "list_name!",
                      matched_cards.is_archived AS "is_archived!",
                      EXISTS (
                          SELECT 1 FROM calendar_events
                          WHERE calendar_events.user_id = $1
                            AND calendar_events.card_id = matched_cards.id
                            AND calendar_events.kind = 'tracked'
                            AND calendar_events.ends_at IS NULL
                      ) AS "is_running!",
                      CASE
                          WHEN to_tsvector('simple'::regconfig, matched_cards.title)
                               @@ search_prefix_query($2)
                              THEN NULL
                          ELSE ts_headline(
                              'simple'::regconfig,
                              matched_cards.notes,
                              search_prefix_query($2),
                              'StartSel="",StopSel="",MaxFragments=1,MaxWords=18,MinWords=8,FragmentDelimiter=" … "'
                          )
                      END AS snippet
               FROM matched_cards
               JOIN boards ON boards.id = matched_cards.board_id AND boards.user_id = $1
               LEFT JOIN board_lists
                 ON board_lists.id = matched_cards.list_id AND board_lists.user_id = $1
               ORDER BY matched_cards.is_archived,
                        matched_cards.match_rank DESC,
                        matched_cards.updated_at DESC,
                        matched_cards.id"#,
            user_id,
            search_words,
            fetch_limit,
        )
        .fetch_all(executor)
        .await
    }

    pub async fn search_single_events(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        search_words: &[String],
        now: UtcInstant,
        fetch_limit: i64,
    ) -> Result<Vec<EventMatch>, sqlx::Error> {
        let event_rows = sqlx::query_as!(
            SingleEventRow,
            r#"SELECT calendar_events.id,
                      calendar_events.title,
                      categories.color AS "category_color?",
                      (extract(epoch FROM calendar_events.starts_at) * 1000)::bigint AS "starts_at_millis!",
                      (extract(epoch FROM calendar_events.ends_at) * 1000)::bigint AS "ends_at_millis!",
                      coalesce(calendar_events.time_zone, users.calendar_time_zone) AS time_zone,
                      open_tasks.id AS "linked_task_id?",
                      ts_rank_cd(calendar_events.search_vector, search_prefix_query($2)) AS "match_rank!"
               FROM calendar_events
               JOIN users ON users.id = calendar_events.user_id
               LEFT JOIN categories
                 ON categories.id = calendar_events.category_id AND categories.user_id = $1
               LEFT JOIN tasks AS open_tasks
                 ON open_tasks.id = calendar_events.task_id
                AND open_tasks.user_id = $1
                AND open_tasks.completed_at IS NULL
               WHERE calendar_events.user_id = $1
                 AND calendar_events.kind = 'planned'
                 AND calendar_events.series_id IS NULL
                 AND calendar_events.search_vector @@ search_prefix_query($2)
               ORDER BY calendar_events.ends_at < 'epoch'::timestamptz + $3::bigint * interval '1 millisecond',
                        "match_rank!" DESC,
                        abs(extract(epoch FROM calendar_events.starts_at) * 1000 - $3::bigint),
                        calendar_events.id
               LIMIT $4"#,
            user_id,
            search_words,
            now.unix_millis(),
            fetch_limit,
        )
        .fetch_all(executor)
        .await?;
        Ok(event_rows
            .into_iter()
            .map(|event_row| EventMatch {
                id: event_row.id,
                series_id: None,
                title: event_row.title,
                category_color: event_row.category_color,
                starts_at: UtcInstant::from_unix_millis(event_row.starts_at_millis),
                ends_at: UtcInstant::from_unix_millis(event_row.ends_at_millis),
                time_zone: event_row.time_zone,
                linked_task_id: event_row.linked_task_id,
                match_rank: event_row.match_rank,
            })
            .collect())
    }

    pub async fn search_series(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        search_words: &[String],
        now: UtcInstant,
        fetch_limit: i64,
    ) -> Result<Vec<MatchedSeries>, sqlx::Error> {
        let series_rows = sqlx::query_as!(
            SeriesRow,
            r#"SELECT event_series.id,
                      event_series.title,
                      event_series.category_id,
                      categories.color AS "category_color?",
                      event_series.time_zone,
                      to_char(event_series.local_start, 'YYYY-MM-DD"T"HH24:MI:SS') AS "local_start_text!",
                      event_series.duration_minutes,
                      event_series.repeat_rule AS "repeat_rule: Json<RepeatRule>",
                      (extract(epoch FROM event_series.until_instant) * 1000)::bigint AS until_instant_millis,
                      coalesce(
                          (SELECT array_agg((extract(epoch FROM event_exceptions.original_start) * 1000)::bigint
                                            ORDER BY event_exceptions.original_start)
                           FROM event_exceptions
                           WHERE event_exceptions.series_id = event_series.id),
                          '{}'
                      ) AS "cancelled_start_millis!",
                      event_series.schedule_id IS NOT NULL AS "is_schedule_block!",
                      ts_rank_cd(event_series.search_vector, search_prefix_query($2)) AS "match_rank!"
               FROM event_series
               LEFT JOIN categories
                 ON categories.id = event_series.category_id AND categories.user_id = $1
               WHERE event_series.user_id = $1
                 AND NOT event_series.hidden
                 AND event_series.search_vector @@ search_prefix_query($2)
               ORDER BY coalesce(
                            event_series.until_instant
                                <= 'epoch'::timestamptz + $3::bigint * interval '1 millisecond',
                            false
                        ),
                        "match_rank!" DESC,
                        event_series.updated_at DESC,
                        event_series.id
               LIMIT $4"#,
            user_id,
            search_words,
            now.unix_millis(),
            fetch_limit,
        )
        .fetch_all(executor)
        .await?;
        series_rows
            .into_iter()
            .map(MatchedSeries::try_from)
            .collect()
    }
}
