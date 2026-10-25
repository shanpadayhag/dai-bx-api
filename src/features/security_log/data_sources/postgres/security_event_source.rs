use std::net::IpAddr;

use sqlx::PgExecutor;
use uuid::Uuid;

use crate::features::security_log::entities::security_event::{
    NewSecurityEvent, SecurityEvent, SecurityEventType,
};

pub struct PostgresSecurityEventSource;

impl PostgresSecurityEventSource {
    pub async fn insert(
        &self,
        executor: impl PgExecutor<'_>,
        new_security_event: &NewSecurityEvent,
    ) -> Result<Uuid, sqlx::Error> {
        let event_id = Uuid::now_v7();
        sqlx::query!(
            "INSERT INTO security_events (id, event_type, subject_ref, ip, user_agent, details)
             VALUES ($1, $2, $3, $4, $5, $6)",
            event_id,
            new_security_event.event_type as SecurityEventType,
            new_security_event.subject_ref,
            new_security_event.client_ip as Option<IpAddr>,
            new_security_event.trimmed_user_agent(),
            new_security_event.details.to_json(),
        )
        .execute(executor)
        .await?;
        Ok(event_id)
    }

    pub async fn find_by_subject_ref(
        &self,
        executor: impl PgExecutor<'_>,
        subject_ref: Uuid,
    ) -> Result<Vec<SecurityEvent>, sqlx::Error> {
        sqlx::query_as!(
            SecurityEvent,
            r#"SELECT id,
                      event_type AS "event_type: SecurityEventType",
                      subject_ref,
                      ip AS "client_ip: IpAddr",
                      user_agent,
                      details
               FROM security_events
               WHERE subject_ref = $1
               ORDER BY id"#,
            subject_ref,
        )
        .fetch_all(executor)
        .await
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::features::security_log::entities::security_event::{
        SecurityEventDetails, SecurityEventType,
    };

    fn new_event_for(subject_ref: Uuid) -> NewSecurityEvent {
        NewSecurityEvent {
            event_type: SecurityEventType::SessionRevoked,
            subject_ref: Some(subject_ref),
            client_ip: Some("180.191.77.129".parse().expect("valid address")),
            user_agent: Some(String::from("Mozilla/5.0 Test")),
            details: SecurityEventDetails::default()
                .with_text("revoked_reason", "signed_out")
                .expect("harmless detail"),
        }
    }

    #[sqlx::test]
    async fn records_and_reads_back_an_event(db_pool: PgPool) {
        let subject_ref = Uuid::now_v7();

        let event_id = PostgresSecurityEventSource
            .insert(&db_pool, &new_event_for(subject_ref))
            .await
            .expect("insert should succeed");
        let recorded_events = PostgresSecurityEventSource
            .find_by_subject_ref(&db_pool, subject_ref)
            .await
            .expect("lookup should succeed");

        assert_eq!(recorded_events.len(), 1);
        assert_eq!(recorded_events[0].id, event_id);
        assert_eq!(
            recorded_events[0].event_type,
            SecurityEventType::SessionRevoked
        );
        assert_eq!(
            recorded_events[0].client_ip,
            Some("180.191.77.129".parse().expect("valid address"))
        );
        assert_eq!(
            recorded_events[0].user_agent.as_deref(),
            Some("Mozilla/5.0 Test")
        );
        assert_eq!(
            recorded_events[0].details,
            serde_json::json!({ "revoked_reason": "signed_out" })
        );
    }

    #[sqlx::test]
    async fn uses_time_ordered_ids(db_pool: PgPool) {
        let subject_ref = Uuid::now_v7();

        let first_event_id = PostgresSecurityEventSource
            .insert(&db_pool, &new_event_for(subject_ref))
            .await
            .expect("insert should succeed");
        let second_event_id = PostgresSecurityEventSource
            .insert(&db_pool, &new_event_for(subject_ref))
            .await
            .expect("insert should succeed");

        assert_eq!(first_event_id.get_version_num(), 7);
        assert!(first_event_id < second_event_id);
    }

    #[sqlx::test]
    async fn keeps_nothing_written_in_a_transaction_that_rolls_back(db_pool: PgPool) {
        let subject_ref = Uuid::now_v7();
        let mut rolled_back_transaction = db_pool.begin().await.expect("transaction should begin");

        PostgresSecurityEventSource
            .insert(&mut *rolled_back_transaction, &new_event_for(subject_ref))
            .await
            .expect("insert should succeed inside the transaction");
        rolled_back_transaction
            .rollback()
            .await
            .expect("rollback should succeed");

        let recorded_events = PostgresSecurityEventSource
            .find_by_subject_ref(&db_pool, subject_ref)
            .await
            .expect("lookup should succeed");
        assert!(recorded_events.is_empty());
    }
}
