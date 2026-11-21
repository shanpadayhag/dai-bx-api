use sqlx::PgPool;
use uuid::Uuid;

use crate::features::activity::data_sources::postgres::activity_source::PostgresActivitySource;
use crate::features::activity::entities::activity_page::{
    ActivityCursor, ActivityPage, ActivityQuery,
};
use crate::features::activity::error::ActivityError;

pub struct ListActivity<'a> {
    db_pool: &'a PgPool,
    postgres_activity_source: PostgresActivitySource,
}

impl<'a> ListActivity<'a> {
    pub fn new(db_pool: &'a PgPool) -> Self {
        Self {
            db_pool,
            postgres_activity_source: PostgresActivitySource,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        activity_query: ActivityQuery,
    ) -> Result<ActivityPage, ActivityError> {
        let mut activity_entries = self
            .postgres_activity_source
            .list(
                self.db_pool,
                user_id,
                activity_query,
                activity_query.limit + 1,
            )
            .await?;
        let has_more = activity_entries.len() as i64 > activity_query.limit;
        activity_entries.truncate(activity_query.limit as usize);
        let next_cursor = has_more
            .then(|| activity_entries.last().map(ActivityCursor::after))
            .flatten();
        Ok(ActivityPage {
            entries: activity_entries,
            next_cursor,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::utc_instant::UtcInstant;
    use crate::features::activity::actions::record_activity::RecordActivity;
    use crate::features::activity::data_sources::postgres::activity_source::tests::seed_subjects;
    use crate::features::activity::entities::activity_entry::{
        ActivityDetails, ActivitySubject, NewActivityEntry,
    };

    #[sqlx::test]
    async fn pages_through_every_entry_with_a_cursor(db_pool: PgPool) {
        let seeded_subjects = seed_subjects(&db_pool, "owner@example.com").await;
        for occurred_millis in [1_000, 2_000, 3_000] {
            RecordActivity::new()
                .execute(
                    &db_pool,
                    NewActivityEntry {
                        user_id: seeded_subjects.user_id,
                        occurred_at: UtcInstant::from_unix_millis(occurred_millis),
                        subject: ActivitySubject::Task(seeded_subjects.task_id),
                        details: ActivityDetails::TaskCompleted { is_subtask: false },
                    },
                )
                .await
                .expect("the entry should be recorded");
        }
        let list_activity = ListActivity::new(&db_pool);

        let first_page = list_activity
            .execute(
                seeded_subjects.user_id,
                ActivityQuery::parse(None, None, Some("2")).expect("the query should parse"),
            )
            .await
            .expect("the first page should load");
        let next_cursor = first_page.next_cursor.expect("a second page should exist");
        let second_page = list_activity
            .execute(
                seeded_subjects.user_id,
                ActivityQuery::parse(None, Some(&next_cursor.to_text()), Some("2"))
                    .expect("the query should parse"),
            )
            .await
            .expect("the second page should load");

        let page_millis = |activity_page: &ActivityPage| {
            activity_page
                .entries
                .iter()
                .map(|activity_entry| activity_entry.occurred_at.unix_millis())
                .collect::<Vec<_>>()
        };
        assert_eq!(page_millis(&first_page), vec![3_000, 2_000]);
        assert_eq!(page_millis(&second_page), vec![1_000]);
        assert_eq!(second_page.next_cursor, None);
    }
}
