use sqlx::PgExecutor;
use uuid::Uuid;

use crate::core::utc_instant::UtcInstant;
use crate::features::activity::entities::activity_entry::{
    ActivityDetails, ActivityEntry, ActivitySubject, NewActivityEntry,
};
use crate::features::activity::entities::activity_page::ActivityQuery;
use crate::features::activity::error::ActivityError;

#[derive(Default)]
pub struct PostgresActivitySource;

impl PostgresActivitySource {
    pub async fn insert(
        &self,
        executor: impl PgExecutor<'_>,
        new_activity_entry: NewActivityEntry,
    ) -> Result<Uuid, sqlx::Error> {
        let (task_id, card_id) = match new_activity_entry.subject {
            ActivitySubject::Task(task_id) => (Some(task_id), None),
            ActivitySubject::Card(card_id) => (None, Some(card_id)),
            ActivitySubject::Nothing => (None, None),
        };
        let (kind, details) = new_activity_entry
            .details
            .into_stored()
            .map_err(|details_error| sqlx::Error::Encode(Box::new(details_error)))?;
        let entry_id = Uuid::now_v7();
        sqlx::query!(
            "INSERT INTO activity_entries
                 (id, user_id, occurred_at, kind, task_id, card_id, title, context_name, details)
             VALUES (
                 $1,
                 $2,
                 'epoch'::timestamptz + $3::bigint * interval '1 millisecond',
                 $4,
                 $5,
                 $6,
                 CASE
                     WHEN $5::uuid IS NOT NULL THEN
                         (SELECT name FROM tasks WHERE id = $5 AND user_id = $2)
                     WHEN $6::uuid IS NOT NULL THEN
                         (SELECT title FROM cards WHERE id = $6 AND user_id = $2)
                 END,
                 CASE
                     WHEN $5::uuid IS NOT NULL THEN
                         (SELECT task_groups.name
                          FROM tasks JOIN task_groups ON task_groups.id = tasks.group_id
                          WHERE tasks.id = $5 AND tasks.user_id = $2)
                     WHEN $6::uuid IS NOT NULL THEN
                         (SELECT boards.name
                          FROM cards JOIN boards ON boards.id = cards.board_id
                          WHERE cards.id = $6 AND cards.user_id = $2)
                 END,
                 $7
             )",
            entry_id,
            new_activity_entry.user_id,
            new_activity_entry.occurred_at.unix_millis(),
            kind,
            task_id,
            card_id,
            details,
        )
        .execute(executor)
        .await?;
        Ok(entry_id)
    }

    pub async fn list(
        &self,
        executor: impl PgExecutor<'_>,
        user_id: Uuid,
        activity_query: ActivityQuery,
        row_limit: i64,
    ) -> Result<Vec<ActivityEntry>, ActivityError> {
        let (cursor_millis, cursor_entry_id) = activity_query
            .cursor
            .map(|cursor| (cursor.occurred_at.unix_millis(), cursor.entry_id))
            .unzip();
        let entry_rows = sqlx::query!(
            r#"SELECT activity_entries.id,
                      (extract(epoch FROM activity_entries.occurred_at) * 1000)::bigint
                          AS "occurred_at_millis!",
                      activity_entries.kind,
                      activity_entries.task_id,
                      activity_entries.card_id,
                      cards.board_id AS "board_id?",
                      activity_entries.title,
                      activity_entries.context_name,
                      activity_entries.details
               FROM activity_entries
               LEFT JOIN cards ON cards.id = activity_entries.card_id
               WHERE activity_entries.user_id = $1
                 AND ($2::bigint IS NULL
                      OR activity_entries.occurred_at
                          >= 'epoch'::timestamptz + $2::bigint * interval '1 millisecond')
                 AND ($3::bigint IS NULL
                      OR (activity_entries.occurred_at, activity_entries.id)
                          < ('epoch'::timestamptz + $3::bigint * interval '1 millisecond', $4::uuid))
               ORDER BY activity_entries.occurred_at DESC, activity_entries.id DESC
               LIMIT $5"#,
            user_id,
            activity_query.since.map(UtcInstant::unix_millis),
            cursor_millis,
            cursor_entry_id,
            row_limit,
        )
        .fetch_all(executor)
        .await?;
        entry_rows
            .into_iter()
            .map(|entry_row| {
                Ok(ActivityEntry {
                    id: entry_row.id,
                    occurred_at: UtcInstant::from_unix_millis(entry_row.occurred_at_millis),
                    title: entry_row.title,
                    context_name: entry_row.context_name,
                    task_id: entry_row.task_id,
                    card_id: entry_row.card_id,
                    board_id: entry_row.board_id,
                    details: ActivityDetails::from_stored(&entry_row.kind, entry_row.details)?,
                })
            })
            .collect()
    }
}

#[cfg(test)]
pub mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::core::request_time_zone::RequestTimeZone;
    use crate::features::activity::entities::activity_entry::TrackedSubject;
    use crate::features::activity::entities::activity_page::ActivityCursor;
    use crate::features::board_lists::actions::create_board_list::CreateBoardList;
    use crate::features::boards::actions::create_board::CreateBoard;
    use crate::features::cards::actions::create_card::CreateCard;
    use crate::features::task_groups::actions::create_task_group::CreateTaskGroup;
    use crate::features::tasks::actions::add_task::AddTask;
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

    async fn seed_task_id(db_pool: &PgPool, user_id: Uuid) -> Uuid {
        let task_group = CreateTaskGroup::new()
            .execute(db_pool, user_id, "DaiBX")
            .await
            .expect("the group should be created");
        AddTask::new(db_pool)
            .execute(
                user_id,
                task_group.id,
                "Write the changelog",
                None,
                &RequestTimeZone::default(),
                UtcInstant::now(),
            )
            .await
            .expect("the task should be added")
            .tasks
            .first()
            .expect("the added task should be listed")
            .id
    }

    async fn seed_card_and_board_ids(db_pool: &PgPool, user_id: Uuid) -> (Uuid, Uuid) {
        let board = CreateBoard::new()
            .execute(db_pool, user_id, "Projects")
            .await
            .expect("the board should be created");
        let board = CreateBoardList::new(db_pool)
            .execute(
                user_id,
                board.board.id,
                "Doing",
                &RequestTimeZone::default(),
                UtcInstant::now(),
            )
            .await
            .expect("the list should be created");
        let list_id = board.lists.first().expect("the list should be listed").id;
        let board = CreateCard::new(db_pool)
            .execute(
                user_id,
                list_id,
                "Plan the release",
                &RequestTimeZone::default(),
                UtcInstant::now(),
            )
            .await
            .expect("the card should be created");
        let card_id = board.cards.first().expect("the card should be listed").id;
        (card_id, board.board.id)
    }

    pub struct SeededSubjects {
        pub user_id: Uuid,
        pub task_id: Uuid,
        pub card_id: Uuid,
        pub board_id: Uuid,
    }

    pub async fn seed_subjects(db_pool: &PgPool, email: &str) -> SeededSubjects {
        let user_id = seed_user_id(db_pool, email).await;
        let task_id = seed_task_id(db_pool, user_id).await;
        let (card_id, board_id) = seed_card_and_board_ids(db_pool, user_id).await;
        SeededSubjects {
            user_id,
            task_id,
            card_id,
            board_id,
        }
    }

    fn started(subject: TrackedSubject) -> ActivityDetails {
        ActivityDetails::Started {
            subject,
            event_id: Uuid::nil().to_string(),
        }
    }

    async fn insert_at(
        db_pool: &PgPool,
        user_id: Uuid,
        subject: ActivitySubject,
        occurred_millis: i64,
    ) -> Uuid {
        PostgresActivitySource
            .insert(
                db_pool,
                NewActivityEntry {
                    user_id,
                    occurred_at: UtcInstant::from_unix_millis(occurred_millis),
                    subject,
                    details: started(TrackedSubject::Task),
                },
            )
            .await
            .expect("the entry should be added")
    }

    async fn list_all(db_pool: &PgPool, user_id: Uuid, activity_query: ActivityQuery) -> Vec<Uuid> {
        PostgresActivitySource
            .list(db_pool, user_id, activity_query, 100)
            .await
            .expect("the list should run")
            .into_iter()
            .map(|activity_entry| activity_entry.id)
            .collect()
    }

    fn default_query() -> ActivityQuery {
        ActivityQuery::parse(None, None, None).expect("defaults should parse")
    }

    #[sqlx::test]
    async fn adds_a_task_entry_with_the_task_and_group_names(db_pool: PgPool) {
        let seeded_subjects = seed_subjects(&db_pool, "owner@example.com").await;
        insert_at(
            &db_pool,
            seeded_subjects.user_id,
            ActivitySubject::Task(seeded_subjects.task_id),
            1_000,
        )
        .await;

        let listed_entries = PostgresActivitySource
            .list(&db_pool, seeded_subjects.user_id, default_query(), 10)
            .await
            .expect("the list should run");

        assert_eq!(listed_entries.len(), 1);
        assert_eq!(
            listed_entries[0].title.as_deref(),
            Some("Write the changelog")
        );
        assert_eq!(listed_entries[0].context_name.as_deref(), Some("DaiBX"));
        assert_eq!(listed_entries[0].task_id, Some(seeded_subjects.task_id));
        assert_eq!(listed_entries[0].board_id, None);
        assert_eq!(listed_entries[0].occurred_at.unix_millis(), 1_000);
        assert_eq!(listed_entries[0].details, started(TrackedSubject::Task));
    }

    #[sqlx::test]
    async fn adds_a_card_entry_with_the_card_and_board_names(db_pool: PgPool) {
        let seeded_subjects = seed_subjects(&db_pool, "owner@example.com").await;
        insert_at(
            &db_pool,
            seeded_subjects.user_id,
            ActivitySubject::Card(seeded_subjects.card_id),
            1_000,
        )
        .await;

        let listed_entries = PostgresActivitySource
            .list(&db_pool, seeded_subjects.user_id, default_query(), 10)
            .await
            .expect("the list should run");

        assert_eq!(listed_entries[0].title.as_deref(), Some("Plan the release"));
        assert_eq!(listed_entries[0].context_name.as_deref(), Some("Projects"));
        assert_eq!(listed_entries[0].card_id, Some(seeded_subjects.card_id));
        assert_eq!(listed_entries[0].board_id, Some(seeded_subjects.board_id));
    }

    #[sqlx::test]
    async fn adds_an_entry_about_nothing_without_names(db_pool: PgPool) {
        let user_id = seed_user_id(&db_pool, "owner@example.com").await;
        PostgresActivitySource
            .insert(
                &db_pool,
                NewActivityEntry {
                    user_id,
                    occurred_at: UtcInstant::from_unix_millis(1_000),
                    subject: ActivitySubject::Nothing,
                    details: ActivityDetails::PlanAccepted { block_count: 3 },
                },
            )
            .await
            .expect("the entry should be added");

        let listed_entries = PostgresActivitySource
            .list(&db_pool, user_id, default_query(), 10)
            .await
            .expect("the list should run");

        assert_eq!(listed_entries[0].title, None);
        assert_eq!(listed_entries[0].context_name, None);
        assert_eq!(
            listed_entries[0].details,
            ActivityDetails::PlanAccepted { block_count: 3 }
        );
    }

    #[sqlx::test]
    async fn refuses_an_entry_about_another_persons_task(db_pool: PgPool) {
        let seeded_subjects = seed_subjects(&db_pool, "owner@example.com").await;
        let other_user_id = seed_user_id(&db_pool, "other@example.com").await;

        let insert_result = PostgresActivitySource
            .insert(
                &db_pool,
                NewActivityEntry {
                    user_id: other_user_id,
                    occurred_at: UtcInstant::from_unix_millis(1_000),
                    subject: ActivitySubject::Task(seeded_subjects.task_id),
                    details: started(TrackedSubject::Task),
                },
            )
            .await;

        assert!(insert_result.is_err());
    }

    #[sqlx::test]
    async fn lists_newest_first_and_only_the_persons_own(db_pool: PgPool) {
        let seeded_subjects = seed_subjects(&db_pool, "owner@example.com").await;
        let other_subjects = seed_subjects(&db_pool, "other@example.com").await;
        let user_id = seeded_subjects.user_id;
        let task = ActivitySubject::Task(seeded_subjects.task_id);
        let older_entry_id = insert_at(&db_pool, user_id, task, 1_000).await;
        let newer_entry_id = insert_at(&db_pool, user_id, task, 2_000).await;
        insert_at(
            &db_pool,
            other_subjects.user_id,
            ActivitySubject::Task(other_subjects.task_id),
            3_000,
        )
        .await;

        let listed_entry_ids = list_all(&db_pool, user_id, default_query()).await;

        assert_eq!(listed_entry_ids, vec![newer_entry_id, older_entry_id]);
    }

    #[sqlx::test]
    async fn lists_only_entries_at_or_after_since(db_pool: PgPool) {
        let seeded_subjects = seed_subjects(&db_pool, "owner@example.com").await;
        let task = ActivitySubject::Task(seeded_subjects.task_id);
        insert_at(&db_pool, seeded_subjects.user_id, task, 1_000).await;
        let on_since_entry_id = insert_at(&db_pool, seeded_subjects.user_id, task, 2_000).await;
        let since_query = ActivityQuery {
            since: Some(UtcInstant::from_unix_millis(2_000)),
            ..default_query()
        };

        let listed_entry_ids = list_all(&db_pool, seeded_subjects.user_id, since_query).await;

        assert_eq!(listed_entry_ids, vec![on_since_entry_id]);
    }

    #[sqlx::test]
    async fn pages_past_entries_that_share_a_time(db_pool: PgPool) {
        let seeded_subjects = seed_subjects(&db_pool, "owner@example.com").await;
        let task = ActivitySubject::Task(seeded_subjects.task_id);
        let mut same_time_entry_ids = Vec::with_capacity(3);
        for _ in 0..3 {
            same_time_entry_ids
                .push(insert_at(&db_pool, seeded_subjects.user_id, task, 5_000).await);
        }
        same_time_entry_ids.sort_unstable_by(|left, right| right.cmp(left));
        let after_first_query = ActivityQuery {
            cursor: Some(ActivityCursor {
                occurred_at: UtcInstant::from_unix_millis(5_000),
                entry_id: same_time_entry_ids[0],
            }),
            ..default_query()
        };

        let listed_entry_ids = list_all(&db_pool, seeded_subjects.user_id, after_first_query).await;

        assert_eq!(listed_entry_ids, same_time_entry_ids[1..].to_vec());
    }
}
