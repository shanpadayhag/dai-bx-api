use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::security_log::data_sources::postgres::security_event_source::PostgresSecurityEventSource;
use crate::features::security_log::entities::security_event::NewSecurityEvent;
use crate::features::security_log::error::SecurityLogError;

pub struct RecordSecurityEvent {
    postgres_security_event_source: PostgresSecurityEventSource,
}

impl Default for RecordSecurityEvent {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordSecurityEvent {
    pub fn new() -> Self {
        Self {
            postgres_security_event_source: PostgresSecurityEventSource,
        }
    }

    pub async fn execute(
        &self,
        executor: impl PgExecutor<'_>,
        new_security_event: &NewSecurityEvent,
    ) -> Result<Uuid, SecurityLogError> {
        Ok(self
            .postgres_security_event_source
            .insert(executor, new_security_event)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::security_log::entities::security_event::{
        SecurityEventDetails, SecurityEventType,
    };

    fn sign_in_failure_for(subject_ref: Uuid) -> NewSecurityEvent {
        NewSecurityEvent {
            event_type: SecurityEventType::SignInFailed,
            subject_ref: Some(subject_ref),
            client_ip: None,
            user_agent: None,
            details: SecurityEventDetails::default(),
        }
    }

    async fn count_events_for(db_pool: &PgPool, subject_ref: Uuid) -> usize {
        PostgresSecurityEventSource
            .find_by_subject_ref(db_pool, subject_ref)
            .await
            .expect("lookup should succeed")
            .len()
    }

    #[sqlx::test]
    async fn records_the_event_with_the_callers_committed_work(db_pool: PgPool) {
        let subject_ref = Uuid::now_v7();
        let mut caller_transaction = db_pool.begin().await.expect("transaction should begin");

        RecordSecurityEvent::new()
            .execute(&mut *caller_transaction, &sign_in_failure_for(subject_ref))
            .await
            .expect("recording should succeed");
        caller_transaction
            .commit()
            .await
            .expect("commit should succeed");

        assert_eq!(count_events_for(&db_pool, subject_ref).await, 1);
    }

    #[sqlx::test]
    async fn drops_the_event_when_the_callers_work_rolls_back(db_pool: PgPool) {
        let subject_ref = Uuid::now_v7();
        let mut caller_transaction = db_pool.begin().await.expect("transaction should begin");

        RecordSecurityEvent::new()
            .execute(&mut *caller_transaction, &sign_in_failure_for(subject_ref))
            .await
            .expect("recording should succeed");
        drop(caller_transaction);

        assert_eq!(count_events_for(&db_pool, subject_ref).await, 0);
    }
}
