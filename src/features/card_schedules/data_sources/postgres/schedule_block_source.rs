use sqlx::PgExecutor;
use sqlx::types::Json;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::card_schedules::entities::card_schedule::{NewScheduleBlock, ScheduleWrite};

#[derive(Default)]
pub struct PostgresScheduleBlockSource;

impl PostgresScheduleBlockSource {
    pub async fn insert(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        schedule_id: Uuid,
        new_block: &NewScheduleBlock,
    ) -> Result<ScheduleWrite, sqlx::Error> {
        let insert_result = match new_block {
            NewScheduleBlock::Series {
                id,
                title,
                category_id,
                time_zone,
                local_start,
                duration_minutes,
                repeat_rule,
            } => {
                sqlx::query!(
                    "INSERT INTO event_series
                         (id, user_id, title, category_id, time_zone, local_start,
                          duration_minutes, repeat_rule, schedule_id)
                     VALUES ($1, $2, $3, $4, $5, $6::text::timestamp, $7, $8, $9)",
                    id,
                    user_id,
                    title,
                    *category_id,
                    time_zone,
                    local_start.to_string(),
                    duration_minutes,
                    Json(repeat_rule) as _,
                    schedule_id,
                )
                .execute(executor)
                .await
            }
            NewScheduleBlock::Event {
                id,
                title,
                category_id,
                time_zone,
                starts_at,
                ends_at,
            } => {
                sqlx::query!(
                    "INSERT INTO calendar_events
                         (id, user_id, kind, title, category_id, starts_at, ends_at,
                          time_zone, schedule_id)
                     VALUES ($1, $2, 'planned', $3, $4,
                             'epoch'::timestamptz + $5::bigint * interval '1 millisecond',
                             'epoch'::timestamptz + $6::bigint * interval '1 millisecond',
                             $7, $8)",
                    id,
                    user_id,
                    title,
                    *category_id,
                    starts_at.unix_millis(),
                    ends_at.unix_millis(),
                    time_zone,
                    schedule_id,
                )
                .execute(executor)
                .await
            }
        };
        match insert_result {
            Ok(_) => Ok(ScheduleWrite::Written),
            Err(sqlx::Error::Database(database_error))
                if database_error.is_foreign_key_violation() =>
            {
                Ok(ScheduleWrite::UnknownLink)
            }
            Err(other_error) => Err(other_error),
        }
    }

    pub async fn end_series(
        &self,
        executor: impl PgExecutor<'_>,
        schedule_ids: &[Uuid],
        now: UtcInstant,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE event_series
             SET until_instant = least(
                     coalesce(until_instant, 'infinity'::timestamptz),
                     'epoch'::timestamptz + $2::bigint * interval '1 millisecond'
                 ),
                 schedule_id = NULL,
                 hidden = false,
                 updated_at = now()
             WHERE schedule_id = ANY($1::uuid[])",
            schedule_ids,
            now.unix_millis(),
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn delete_future_events(
        &self,
        executor: impl PgExecutor<'_>,
        schedule_ids: &[Uuid],
        now: UtcInstant,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM calendar_events
             WHERE schedule_id = ANY($1::uuid[])
               AND starts_at > 'epoch'::timestamptz + $2::bigint * interval '1 millisecond'",
            schedule_ids,
            now.unix_millis(),
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn release_events(
        &self,
        executor: impl PgExecutor<'_>,
        schedule_ids: &[Uuid],
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE calendar_events SET schedule_id = NULL, updated_at = now() WHERE schedule_id = ANY($1::uuid[])",
            schedule_ids,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn set_series_hidden(
        &self,
        executor: impl PgExecutor<'_>,
        schedule_ids: &[Uuid],
        is_hidden: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE event_series SET hidden = $2, updated_at = now() WHERE schedule_id = ANY($1::uuid[])",
            schedule_ids,
            is_hidden,
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
    use crate::features::card_schedules::data_sources::postgres::card_schedule_source::tests::{
        instant, new_schedule, seed_card, seed_category_id, seed_schedule, seed_user_id,
        weekly_settings,
    };

    struct BlockRows {
        series: Vec<(Option<Uuid>, Option<i64>, bool, String)>,
        events: Vec<(Option<Uuid>, i64)>,
    }

    async fn block_rows(db_pool: &PgPool, user_id: Uuid) -> BlockRows {
        let series = sqlx::query!(
            r#"SELECT schedule_id,
                      (extract(epoch FROM until_instant) * 1000)::bigint AS until_millis,
                      hidden,
                      to_char(local_start, 'YYYY-MM-DD"T"HH24:MI') AS "local_start_text!"
               FROM event_series WHERE user_id = $1 ORDER BY id"#,
            user_id,
        )
        .fetch_all(db_pool)
        .await
        .expect("the read should run")
        .into_iter()
        .map(|series_row| {
            (
                series_row.schedule_id,
                series_row.until_millis,
                series_row.hidden,
                series_row.local_start_text,
            )
        })
        .collect();
        let events = sqlx::query!(
            r#"SELECT schedule_id,
                      (extract(epoch FROM starts_at) * 1000)::bigint AS "starts_at_millis!"
               FROM calendar_events WHERE user_id = $1 ORDER BY starts_at"#,
            user_id,
        )
        .fetch_all(db_pool)
        .await
        .expect("the read should run")
        .into_iter()
        .map(|event_row| (event_row.schedule_id, event_row.starts_at_millis))
        .collect();
        BlockRows { series, events }
    }

    fn block_event(title: &str, starts_at: &str) -> NewScheduleBlock {
        NewScheduleBlock::Event {
            id: Uuid::now_v7(),
            title: title.to_owned(),
            category_id: None,
            time_zone: String::from("UTC"),
            starts_at: instant(starts_at),
            ends_at: UtcInstant::from_unix_millis(instant(starts_at).unix_millis() + 1_800_000),
        }
    }

    fn block_series(category_id: Option<Uuid>) -> NewScheduleBlock {
        NewScheduleBlock::Series {
            id: Uuid::now_v7(),
            title: String::from("Weekly review"),
            category_id,
            time_zone: String::from("Asia/Manila"),
            local_start: "2030-01-07T18:00:00".parse().expect("a local time"),
            duration_minutes: 60,
            repeat_rule: weekly_settings(Uuid::now_v7())
                .timing
                .repeat_rule
                .expect("a weekly rule"),
        }
    }

    #[sqlx::test]
    async fn hides_shows_and_ends_a_schedule_s_blocks(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let schedule_id = seed_schedule(
            &db_pool,
            &new_schedule(&seeded_card, weekly_settings(seeded_card.to_do_list_id)),
        )
        .await;
        for new_block in [
            block_series(None),
            block_event("Past block", "2026-11-01T09:00:00Z"),
            block_event("Future block", "2030-01-09T09:00:00Z"),
        ] {
            let block_write = PostgresScheduleBlockSource
                .insert(&db_pool, user_id, schedule_id, &new_block)
                .await
                .expect("the insert should run");
            assert_eq!(block_write, ScheduleWrite::Written);
        }
        let now = instant("2026-11-08T12:00:00Z");

        PostgresScheduleBlockSource
            .set_series_hidden(&db_pool, &[schedule_id], true)
            .await
            .expect("the hide should run");
        let hidden_rows = block_rows(&db_pool, user_id).await;
        PostgresScheduleBlockSource
            .delete_future_events(&db_pool, &[schedule_id], now)
            .await
            .expect("the delete should run");
        PostgresScheduleBlockSource
            .end_series(&db_pool, &[schedule_id], now)
            .await
            .expect("the end should run");
        PostgresScheduleBlockSource
            .release_events(&db_pool, &[schedule_id])
            .await
            .expect("the release should run");
        let ended_rows = block_rows(&db_pool, user_id).await;

        assert_eq!(
            hidden_rows.series,
            vec![(
                Some(schedule_id),
                None,
                true,
                String::from("2030-01-07T18:00")
            )]
        );
        assert_eq!(
            ended_rows.series,
            vec![(
                None,
                Some(now.unix_millis()),
                false,
                String::from("2030-01-07T18:00")
            )]
        );
        assert_eq!(
            ended_rows.events,
            vec![(None, instant("2026-11-01T09:00:00Z").unix_millis())]
        );
    }

    #[sqlx::test]
    async fn reports_another_person_s_block_category_as_an_unknown_link(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;
        let other_category_id = seed_category_id(&db_pool, other_user_id).await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let schedule_id = seed_schedule(
            &db_pool,
            &new_schedule(&seeded_card, weekly_settings(seeded_card.to_do_list_id)),
        )
        .await;

        let block_write = PostgresScheduleBlockSource
            .insert(
                &db_pool,
                user_id,
                schedule_id,
                &block_series(Some(other_category_id)),
            )
            .await
            .expect("the insert should run");

        assert_eq!(block_write, ScheduleWrite::UnknownLink);
    }
}
