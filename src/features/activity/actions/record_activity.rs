use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::activity::data_sources::postgres::activity_source::PostgresActivitySource;
use crate::features::activity::entities::activity_entry::NewActivityEntry;

#[derive(Default)]
pub struct RecordActivity {
    postgres_activity_source: PostgresActivitySource,
}

impl RecordActivity {
    pub fn new() -> Self {
        Self {
            postgres_activity_source: PostgresActivitySource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        new_activity_entry: NewActivityEntry,
    ) -> Result<Uuid, sqlx::Error> {
        self.postgres_activity_source
            .insert(executor, new_activity_entry)
            .await
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::core::utc_instant::UtcInstant;
    use crate::features::activity::data_sources::postgres::activity_source::tests::seed_subjects;
    use crate::features::activity::entities::activity_entry::{ActivityDetails, ActivitySubject};
    use crate::features::activity::entities::activity_page::ActivityQuery;

    fn completed_entry(user_id: Uuid, task_id: Uuid) -> NewActivityEntry {
        NewActivityEntry {
            user_id,
            occurred_at: UtcInstant::now(),
            subject: ActivitySubject::Task(task_id),
            details: ActivityDetails::TaskCompleted { is_subtask: false },
        }
    }

    async fn entry_count(db_pool: &PgPool, user_id: Uuid) -> usize {
        PostgresActivitySource
            .list(
                db_pool,
                user_id,
                ActivityQuery::parse(None, None, None).expect("defaults should parse"),
                100,
            )
            .await
            .expect("the list should run")
            .len()
    }

    #[sqlx::test]
    async fn records_the_entry_with_the_callers_committed_work(db_pool: PgPool) {
        let seeded_subjects = seed_subjects(&db_pool, "owner@example.com").await;
        let mut caller_transaction = db_pool.begin().await.expect("transaction should begin");

        RecordActivity::new()
            .execute(
                &mut *caller_transaction,
                completed_entry(seeded_subjects.user_id, seeded_subjects.task_id),
            )
            .await
            .expect("the entry should be recorded");
        caller_transaction
            .commit()
            .await
            .expect("commit should succeed");

        assert_eq!(entry_count(&db_pool, seeded_subjects.user_id).await, 1);
    }

    #[sqlx::test]
    async fn drops_the_entry_when_the_callers_work_rolls_back(db_pool: PgPool) {
        let seeded_subjects = seed_subjects(&db_pool, "owner@example.com").await;
        let mut caller_transaction = db_pool.begin().await.expect("transaction should begin");

        RecordActivity::new()
            .execute(
                &mut *caller_transaction,
                completed_entry(seeded_subjects.user_id, seeded_subjects.task_id),
            )
            .await
            .expect("the entry should be recorded");
        caller_transaction
            .rollback()
            .await
            .expect("rollback should succeed");

        assert_eq!(entry_count(&db_pool, seeded_subjects.user_id).await, 0);
    }
}
