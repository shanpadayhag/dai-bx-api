use sqlx::PgExecutor;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::card_schedules::entities::schedule_run::{
    NewScheduleRun, RunPage, RunTiming, ScheduleRun,
};

#[derive(Default)]
pub struct PostgresScheduleRunSource;

impl PostgresScheduleRunSource {
    pub async fn insert(
        &self,
        executor: impl PgExecutor<'_>,
        new_run: &NewScheduleRun,
    ) -> Result<bool, sqlx::Error> {
        let insert_result = sqlx::query!(
            "INSERT INTO schedule_runs (id, user_id, schedule_id, occurrence_at, ran_at, how, moved)
             VALUES ($1, $2, $3,
                     'epoch'::timestamptz + $4::bigint * interval '1 millisecond',
                     'epoch'::timestamptz + $5::bigint * interval '1 millisecond',
                     $6, false)
             ON CONFLICT DO NOTHING",
            new_run.id,
            new_run.user_id,
            new_run.schedule_id,
            new_run.occurrence_at.unix_millis(),
            new_run.ran_at.unix_millis(),
            new_run.timing.as_str(),
        )
        .execute(executor)
        .await?;
        Ok(insert_result.rows_affected() == 1)
    }

    pub async fn mark_moved(
        &self,
        executor: impl PgExecutor<'_>,
        run_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE schedule_runs SET moved = true WHERE id = $1",
            run_id
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn list_page(
        &self,
        executor: impl PgExecutor<'_>,
        schedule_id: Uuid,
        card_id: Uuid,
        run_page: RunPage,
        now: UtcInstant,
    ) -> Result<Vec<ScheduleRun>, sqlx::Error> {
        let run_rows = sqlx::query!(
            r#"WITH ordered_runs AS (
                   SELECT id, ran_at, how, moved,
                          coalesce(
                              lead(ran_at) OVER (ORDER BY ran_at, id),
                              'epoch'::timestamptz + $5::bigint * interval '1 millisecond'
                          ) AS window_end
                   FROM schedule_runs
                   WHERE schedule_id = $1
               )
               SELECT ordered_runs.id,
                      (extract(epoch FROM ordered_runs.ran_at) * 1000)::bigint AS "ran_at_millis!",
                      ordered_runs.how,
                      ordered_runs.moved,
                      coalesce(
                          (SELECT floor(sum(extract(epoch FROM
                                      least(
                                          coalesce(calendar_events.ends_at,
                                                   'epoch'::timestamptz + $5::bigint * interval '1 millisecond'),
                                          ordered_runs.window_end
                                      ) - greatest(calendar_events.starts_at, ordered_runs.ran_at)
                                  )) / 60)::bigint
                           FROM calendar_events
                           WHERE calendar_events.card_id = $2
                             AND calendar_events.kind = 'tracked'
                             AND calendar_events.starts_at < ordered_runs.window_end
                             AND coalesce(calendar_events.ends_at,
                                          'epoch'::timestamptz + $5::bigint * interval '1 millisecond')
                                 > ordered_runs.ran_at),
                          0
                      ) AS "tracked_minutes!"
               FROM ordered_runs
               WHERE $3::bigint IS NULL
                  OR ordered_runs.ran_at < 'epoch'::timestamptz + $3::bigint * interval '1 millisecond'
               ORDER BY ordered_runs.ran_at DESC, ordered_runs.id DESC
               LIMIT $4"#,
            schedule_id,
            card_id,
            run_page.before.map(UtcInstant::unix_millis),
            run_page.limit,
            now.unix_millis(),
        )
        .fetch_all(executor)
        .await?;
        Ok(run_rows
            .into_iter()
            .map(|run_row| ScheduleRun {
                id: run_row.id,
                ran_at: UtcInstant::from_unix_millis(run_row.ran_at_millis),
                timing: RunTiming::from_stored(&run_row.how),
                is_moved: run_row.moved,
                tracked_minutes: run_row.tracked_minutes,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::card_schedules::data_sources::postgres::card_schedule_source::tests::{
        SeededCard, instant, new_schedule, seed_card, seed_schedule, seed_user_id, weekly_settings,
    };

    async fn seed_run(
        db_pool: &PgPool,
        seeded_card: &SeededCard,
        schedule_id: Uuid,
        ran_at: &str,
        timing: RunTiming,
    ) -> bool {
        PostgresScheduleRunSource
            .insert(
                db_pool,
                &NewScheduleRun {
                    id: Uuid::now_v7(),
                    user_id: seeded_card.user_id,
                    schedule_id,
                    occurrence_at: instant(ran_at),
                    ran_at: instant(ran_at),
                    timing,
                },
            )
            .await
            .expect("the insert should run")
    }

    async fn seed_tracked_event(
        db_pool: &PgPool,
        seeded_card: &SeededCard,
        starts_at: &str,
        ends_at: Option<&str>,
    ) {
        sqlx::query!(
            "INSERT INTO calendar_events (id, user_id, kind, title, card_id, starts_at, ends_at, finished_at)
             VALUES ($1, $2, 'tracked', 'Weekly review', $3,
                     'epoch'::timestamptz + $4::bigint * interval '1 millisecond',
                     'epoch'::timestamptz + $5::bigint * interval '1 millisecond',
                     'epoch'::timestamptz + $5::bigint * interval '1 millisecond')",
            Uuid::now_v7(),
            seeded_card.user_id,
            seeded_card.card_id,
            instant(starts_at).unix_millis(),
            ends_at.map(|ends_at| instant(ends_at).unix_millis()),
        )
        .execute(db_pool)
        .await
        .expect("the tracked event should be inserted");
    }

    #[sqlx::test]
    async fn runs_each_scheduled_occurrence_once_but_every_press_by_hand(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let schedule_id = seed_schedule(
            &db_pool,
            &new_schedule(&seeded_card, weekly_settings(seeded_card.to_do_list_id)),
        )
        .await;

        let inserted = [
            seed_run(
                &db_pool,
                &seeded_card,
                schedule_id,
                "2030-01-07T10:00:00Z",
                RunTiming::OnTime,
            )
            .await,
            seed_run(
                &db_pool,
                &seeded_card,
                schedule_id,
                "2030-01-07T10:00:00Z",
                RunTiming::Late,
            )
            .await,
            seed_run(
                &db_pool,
                &seeded_card,
                schedule_id,
                "2030-01-07T10:00:00Z",
                RunTiming::ByHand,
            )
            .await,
            seed_run(
                &db_pool,
                &seeded_card,
                schedule_id,
                "2030-01-07T10:00:00Z",
                RunTiming::ByHand,
            )
            .await,
        ];

        assert_eq!(inserted, [true, false, true, true]);
    }

    #[sqlx::test]
    async fn splits_tracked_time_between_the_runs_it_spans(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        let seeded_card = seed_card(&db_pool, user_id).await;
        let other_card = seed_card(&db_pool, user_id).await;
        let schedule_id = seed_schedule(
            &db_pool,
            &new_schedule(&seeded_card, weekly_settings(seeded_card.to_do_list_id)),
        )
        .await;
        for ran_at in ["2030-01-07T10:00:00Z", "2030-01-14T10:00:00Z"] {
            seed_run(
                &db_pool,
                &seeded_card,
                schedule_id,
                ran_at,
                RunTiming::OnTime,
            )
            .await;
        }
        seed_tracked_event(
            &db_pool,
            &seeded_card,
            "2030-01-07T09:30:00Z",
            Some("2030-01-07T11:00:00Z"),
        )
        .await;
        seed_tracked_event(
            &db_pool,
            &seeded_card,
            "2030-01-14T09:00:00Z",
            Some("2030-01-14T10:45:30Z"),
        )
        .await;
        seed_tracked_event(&db_pool, &seeded_card, "2030-01-15T08:00:00Z", None).await;
        seed_tracked_event(
            &db_pool,
            &other_card,
            "2030-01-08T08:00:00Z",
            Some("2030-01-08T12:00:00Z"),
        )
        .await;

        let listed_runs = PostgresScheduleRunSource
            .list_page(
                &db_pool,
                schedule_id,
                seeded_card.card_id,
                RunPage {
                    before: None,
                    limit: 20,
                },
                instant("2030-01-15T08:20:00Z"),
            )
            .await
            .expect("the list should run");
        let older_page = PostgresScheduleRunSource
            .list_page(
                &db_pool,
                schedule_id,
                seeded_card.card_id,
                RunPage {
                    before: Some(instant("2030-01-14T10:00:00Z")),
                    limit: 1,
                },
                instant("2030-01-15T08:20:00Z"),
            )
            .await
            .expect("the list should run");

        let tracked_minutes: Vec<i64> = listed_runs
            .iter()
            .map(|listed_run| listed_run.tracked_minutes)
            .collect();
        assert_eq!(tracked_minutes, vec![65, 120]);
        assert_eq!(listed_runs[0].ran_at, instant("2030-01-14T10:00:00Z"));
        assert_eq!(older_page.len(), 1);
        assert_eq!(older_page[0].ran_at, instant("2030-01-07T10:00:00Z"));
        assert!(!listed_runs[0].is_moved);
    }
}
